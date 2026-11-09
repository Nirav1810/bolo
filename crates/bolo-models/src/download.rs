//! Resumable, verified model downloads.
//!
//! Flow: `<root>/.downloads/<id>.part` (HTTP Range resume) → SHA-256 verify →
//! unpack into `<root>/.staging-<id>` → atomic rename to `<root>/<id>` → write marker.
//! Dropping the future cancels; the `.part` file is kept so the next attempt resumes.

use std::io::{Read, Seek};
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::manifest::{Download, ModelEntry};
use crate::{ModelError, Result};

/// Written last; a model dir without it is treated as not installed.
pub const INSTALLED_MARKER: &str = ".bolo-installed";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Downloading,
    Verifying,
    Unpacking,
    Done,
}

#[derive(Debug, Clone, Copy)]
pub struct DownloadProgress {
    pub phase: Phase,
    pub downloaded: u64,
    pub total: u64,
}

pub struct Downloader {
    client: reqwest::Client,
    root: PathBuf,
}

impl Downloader {
    /// `client` should be the app's shared client (see `bolo_stt::cloud::http_client`).
    pub fn new(client: reqwest::Client, models_root: impl Into<PathBuf>) -> Self {
        Self { client, root: models_root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Download, verify and install `entry`. Returns the install directory.
    pub async fn install(&self, entry: &ModelEntry, progress: &(dyn Fn(DownloadProgress) + Sync)) -> Result<PathBuf> {
        let dir = entry.install_dir(&self.root);
        if entry.is_installed(&self.root) {
            return Ok(dir);
        }
        let dl = entry
            .download
            .as_ref()
            .ok_or_else(|| ModelError::NotAvailable(entry.id.clone(), format!("{:?}", entry.status)))?;

        let tmp = self.root.join(".downloads");
        tokio::fs::create_dir_all(&tmp).await?;
        let part = tmp.join(format!("{}.part", entry.id));

        self.fetch(dl, &part, progress).await?;

        progress(DownloadProgress { phase: Phase::Verifying, downloaded: dl.size_bytes, total: dl.size_bytes });
        let actual = sha256_file(part.clone()).await?;
        if !actual.eq_ignore_ascii_case(&dl.sha256) {
            tokio::fs::remove_file(&part).await.ok();
            return Err(ModelError::Checksum { file: dl.url.clone(), expected: dl.sha256.clone(), actual });
        }

        progress(DownloadProgress { phase: Phase::Unpacking, downloaded: dl.size_bytes, total: dl.size_bytes });
        let (root, entry_c, dl_c, part_c) = (self.root.clone(), entry.clone(), dl.clone(), part.clone());
        tokio::task::spawn_blocking(move || unpack(&root, &entry_c, &dl_c, &part_c))
            .await
            .map_err(|e| ModelError::Io(std::io::Error::other(e)))??;
        tokio::fs::remove_file(&part).await.ok();

        if !entry.is_installed(&self.root) {
            return Err(ModelError::Missing(dir.display().to_string()));
        }
        progress(DownloadProgress { phase: Phase::Done, downloaded: dl.size_bytes, total: dl.size_bytes });
        tracing::info!(model = %entry.id, "model installed");
        Ok(dir)
    }

    /// Delete an installed model.
    pub async fn remove(&self, entry: &ModelEntry) -> Result<()> {
        let dir = entry.install_dir(&self.root);
        if dir.exists() {
            tokio::fs::remove_dir_all(dir).await?;
        }
        Ok(())
    }

    async fn fetch(&self, dl: &Download, part: &Path, progress: &(dyn Fn(DownloadProgress) + Sync)) -> Result<()> {
        let mut have = tokio::fs::metadata(part).await.map(|m| m.len()).unwrap_or(0);
        if have > dl.size_bytes {
            tokio::fs::remove_file(part).await?;
            have = 0;
        }
        if have == dl.size_bytes {
            return Ok(());
        }

        let mut resp = match self.get_from(&dl.url, None, have).await {
            Ok(r) if !r.status().is_server_error() => r,
            primary => {
                // github.com/<o>/<r>/releases/download/... sometimes 5xx's while the API
                // asset route still works. Same bytes, and we verify SHA-256 anyway.
                let Some(api) = self.github_api_asset_url(&dl.url).await else {
                    return primary.and_then(|r| Err(ModelError::Status(r.status().as_u16())));
                };
                tracing::warn!(url = %dl.url, "primary download failed, using GitHub API asset route");
                self.get_from(&api, Some("application/octet-stream"), have).await?
            }
        };
        let status = resp.status();
        let append = match status {
            reqwest::StatusCode::PARTIAL_CONTENT => true,
            s if s.is_success() => {
                have = 0; // server ignored Range: start over
                false
            }
            s => return Err(ModelError::Status(s.as_u16())),
        };

        let mut file =
            tokio::fs::OpenOptions::new().create(true).write(true).append(append).truncate(!append).open(part).await?;
        let mut done = have;
        let mut last_report = 0u64;
        while let Some(chunk) = resp.chunk().await.map_err(net)? {
            file.write_all(&chunk).await?;
            done += chunk.len() as u64;
            if done - last_report >= 1 << 20 || done == dl.size_bytes {
                last_report = done;
                progress(DownloadProgress { phase: Phase::Downloading, downloaded: done, total: dl.size_bytes });
            }
        }
        file.flush().await?;
        if done != dl.size_bytes {
            return Err(ModelError::Network(format!("incomplete download: {done} of {} bytes", dl.size_bytes)));
        }
        Ok(())
    }
}

impl Downloader {
    async fn get_from(&self, url: &str, accept: Option<&str>, have: u64) -> Result<reqwest::Response> {
        let mut req = self.client.get(url);
        if let Some(a) = accept {
            req = req.header(reqwest::header::ACCEPT, a);
        }
        if have > 0 {
            req = req.header(reqwest::header::RANGE, format!("bytes={have}-"));
        }
        req.send().await.map_err(net)
    }

    /// Map `https://github.com/<o>/<r>/releases/download/<tag>/<name>` to its API asset URL.
    async fn github_api_asset_url(&self, url: &str) -> Option<String> {
        let (owner, repo, tag, name) = parse_github_release_url(url)?;
        let api = format!("https://api.github.com/repos/{owner}/{repo}/releases/tags/{tag}");
        let resp =
            self.client.get(&api).header(reqwest::header::ACCEPT, "application/vnd.github+json").send().await.ok()?;
        let release: serde_json::Value = resp.error_for_status().ok()?.json().await.ok()?;
        release["assets"]
            .as_array()?
            .iter()
            .find(|a| a["name"].as_str() == Some(name))
            .and_then(|a| a["url"].as_str().map(str::to_owned))
    }
}

fn parse_github_release_url(url: &str) -> Option<(&str, &str, &str, &str)> {
    let rest = url.strip_prefix("https://github.com/")?;
    let mut it = rest.splitn(6, '/');
    let (owner, repo) = (it.next()?, it.next()?);
    if it.next()? != "releases" || it.next()? != "download" {
        return None;
    }
    let (tag, name) = (it.next()?, it.next()?);
    (!name.contains('/')).then_some((owner, repo, tag, name))
}

fn net(e: reqwest::Error) -> ModelError {
    ModelError::Network(e.to_string())
}

async fn sha256_file(path: PathBuf) -> Result<String> {
    tokio::task::spawn_blocking(move || -> Result<String> {
        let mut f = std::fs::File::open(path)?;
        let mut h = Sha256::new();
        let mut buf = vec![0u8; 1 << 20];
        loop {
            let n = f.read(&mut buf)?;
            if n == 0 {
                break;
            }
            h.update(&buf[..n]);
        }
        Ok(hex::encode(h.finalize()))
    })
    .await
    .map_err(|e| ModelError::Io(std::io::Error::other(e)))?
}

fn unpack(root: &Path, entry: &ModelEntry, dl: &Download, part: &Path) -> Result<()> {
    let dest = entry.install_dir(root);
    let staging = root.join(format!(".staging-{}", entry.id));
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    std::fs::create_dir_all(&staging)?;

    let unpacked_dir = match (dl.archive.as_deref(), dl.root_dir.as_deref()) {
        (Some("tar.bz2"), Some(root_dir)) => {
            let mut f = std::fs::File::open(part)?;
            f.rewind()?;
            let mut ar = tar::Archive::new(bzip2::read::BzDecoder::new(std::io::BufReader::new(f)));
            // `unpack` refuses entries that escape `staging` (absolute paths, `..`).
            ar.unpack(&staging)?;
            staging.join(root_dir)
        }
        (None, _) => {
            let name = entry.files.values().next().ok_or_else(|| ModelError::Missing(entry.id.clone()))?;
            std::fs::copy(part, staging.join(name))?;
            staging.clone()
        }
        (Some(other), _) => return Err(ModelError::Manifest(format!("unsupported archive type {other}"))),
    };

    if dest.exists() {
        std::fs::remove_dir_all(&dest)?;
    }
    std::fs::rename(&unpacked_dir, &dest)?;
    std::fs::write(dest.join(INSTALLED_MARKER), &dl.sha256)?;
    if staging.exists() {
        std::fs::remove_dir_all(&staging)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_github_release_urls() {
        assert_eq!(
            parse_github_release_url(
                "https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models/silero_vad.onnx"
            ),
            Some(("k2-fsa", "sherpa-onnx", "asr-models", "silero_vad.onnx"))
        );
        assert_eq!(parse_github_release_url("https://huggingface.co/x/y/resolve/main/f"), None);
    }

    #[tokio::test]
    async fn sha256_of_known_content() {
        let dir = std::env::temp_dir().join(format!("bolo-models-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("abc.txt");
        std::fs::write(&p, b"abc").unwrap();
        assert_eq!(sha256_file(p).await.unwrap(), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        std::fs::remove_dir_all(&dir).ok();
    }
}
