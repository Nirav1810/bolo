//! The engine config: defaults plus `BOLO_*` environment variables, until settings land
//! in `bolo-store` (ticket 02).
//!
//! | Variable | Value |
//! |---|---|
//! | `BOLO_ENGINES` | comma-separated engine specs, e.g. `local:nemotron-stream,deepgram` |
//! | `BOLO_LANGUAGE` | `auto`, `english` (`en`) or `hinglish` (`hi`) |
//! | `BOLO_SCRIPT` | `roman` or `devanagari` |
//! | `BOLO_POLISH` | `off`, `on`, or a tone (`neutral`, `casual`, `formal`) for on |
//! | `BOLO_DICTIONARY` | comma-separated terms |
//! | `BOLO_MODELS_DIR` | overrides the ADR-007 models directory |
//! | `<PROVIDER>_API_KEY` | keys for `deepgram`, `groq`, `sarvam`, `anthropic` |

use std::path::PathBuf;

use bolo_core::{ANTHROPIC, EngineConfig, HinglishScript, LanguageMode, Polish};

const PROVIDERS: [&str; 4] = ["deepgram", "groq", "sarvam", ANTHROPIC];

/// The engine config from `var` (a lookup like [`std::env::var`]) on top of the
/// defaults. `models_dir` is the ADR-007 location, used unless `BOLO_MODELS_DIR` is set.
pub fn engine_config(var: impl Fn(&str) -> Option<String>, models_dir: PathBuf) -> Result<EngineConfig, String> {
    let var = |name: &str| var(name).map(|v| v.trim().to_owned()).filter(|v| !v.is_empty());
    let list = |v: String| v.split(',').map(str::trim).filter(|s| !s.is_empty()).map(String::from).collect::<Vec<_>>();

    let mut config = EngineConfig { models_dir, ..Default::default() };
    if let Some(engines) = var("BOLO_ENGINES").map(list).filter(|l| !l.is_empty()) {
        config.engines = engines;
    }
    if let Some(v) = var("BOLO_LANGUAGE") {
        config.language = match v.to_lowercase().as_str() {
            "auto" => LanguageMode::Auto,
            "english" | "en" => LanguageMode::English,
            "hinglish" | "hi" => LanguageMode::Hinglish,
            _ => return Err(format!("BOLO_LANGUAGE={v}: use auto, english or hinglish")),
        };
    }
    if let Some(v) = var("BOLO_SCRIPT") {
        config.script = match v.to_lowercase().as_str() {
            "roman" => HinglishScript::Roman,
            "devanagari" => HinglishScript::Devanagari,
            _ => return Err(format!("BOLO_SCRIPT={v}: use roman or devanagari")),
        };
    }
    if let Some(terms) = var("BOLO_DICTIONARY").map(list) {
        config.dictionary = terms;
    }
    if let Some(dir) = var("BOLO_MODELS_DIR") {
        config.models_dir = PathBuf::from(dir);
    }
    for p in PROVIDERS {
        if let Some(key) = var(&format!("{}_API_KEY", p.to_uppercase())) {
            config.api_keys.set(p, key);
        }
    }
    config.polish = match var("BOLO_POLISH").map(|v| v.to_lowercase()).as_deref() {
        None | Some("off") => Polish::Off,
        Some(tone) => {
            if config.api_keys.get(ANTHROPIC).is_none() {
                return Err("BOLO_POLISH needs ANTHROPIC_API_KEY".into());
            }
            let tone = if tone == "on" { "neutral" } else { tone };
            Polish::On { tone: tone.into(), app: String::new(), dictionary: config.dictionary.clone() }
        }
    };
    Ok(config)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use bolo_core::{EngineConfig, HinglishScript, LanguageMode, Polish};

    use super::engine_config;

    fn config(vars: &[(&str, &str)]) -> Result<EngineConfig, String> {
        let vars: HashMap<String, String> = vars.iter().map(|(k, v)| ((*k).into(), (*v).into())).collect();
        engine_config(|k| vars.get(k).cloned(), PathBuf::from("models"))
    }

    #[test]
    fn no_variables_gives_the_defaults_and_the_adr_007_models_dir() {
        let c = config(&[]).unwrap();
        let d = EngineConfig::default();
        assert_eq!(c.engines, d.engines);
        assert_eq!(c.language, LanguageMode::Auto);
        assert_eq!(c.script, HinglishScript::Roman);
        assert_eq!(c.polish, Polish::Off);
        assert_eq!(c.models_dir, PathBuf::from("models"));
        assert!(c.dictionary.is_empty());
    }

    #[test]
    fn variables_override_the_defaults() {
        let c = config(&[
            ("BOLO_ENGINES", " local:parakeet-v2-en , deepgram ,"),
            ("BOLO_LANGUAGE", "Hi"),
            ("BOLO_SCRIPT", "devanagari"),
            ("BOLO_DICTIONARY", "Nirav, Bolo"),
            ("BOLO_MODELS_DIR", "D:\\models"),
            ("DEEPGRAM_API_KEY", "dg"),
            ("GROQ_API_KEY", "  "),
        ])
        .unwrap();
        assert_eq!(c.engines, ["local:parakeet-v2-en", "deepgram"]);
        assert_eq!(c.language, LanguageMode::Hinglish);
        assert_eq!(c.script, HinglishScript::Devanagari);
        assert_eq!(c.dictionary, ["Nirav", "Bolo"]);
        assert_eq!(c.models_dir, PathBuf::from("D:\\models"));
        assert_eq!(c.api_keys.get("deepgram"), Some("dg"));
        assert_eq!(c.api_keys.get("groq"), None);
    }

    #[test]
    fn empty_values_count_as_unset() {
        let c = config(&[("BOLO_ENGINES", " , "), ("BOLO_LANGUAGE", ""), ("BOLO_POLISH", " ")]).unwrap();
        assert_eq!(c.engines, EngineConfig::default().engines);
        assert_eq!(c.polish, Polish::Off);
    }

    #[test]
    fn polish_needs_the_anthropic_key_and_takes_a_tone() {
        assert!(config(&[("BOLO_POLISH", "on")]).unwrap_err().contains("ANTHROPIC_API_KEY"));
        let on = config(&[("BOLO_POLISH", "on"), ("ANTHROPIC_API_KEY", "k"), ("BOLO_DICTIONARY", "Bolo")]).unwrap();
        assert_eq!(
            on.polish,
            Polish::On { tone: "neutral".into(), app: String::new(), dictionary: vec!["Bolo".into()] }
        );
        let casual = config(&[("BOLO_POLISH", "Casual"), ("ANTHROPIC_API_KEY", "k")]).unwrap();
        assert!(matches!(casual.polish, Polish::On { tone, .. } if tone == "casual"));
    }

    #[test]
    fn bad_values_are_reported_with_the_variable() {
        assert!(config(&[("BOLO_LANGUAGE", "french")]).unwrap_err().starts_with("BOLO_LANGUAGE=french"));
        assert!(config(&[("BOLO_SCRIPT", "latin")]).unwrap_err().starts_with("BOLO_SCRIPT=latin"));
    }
}
