#!/usr/bin/env python3
"""Build crates/bolo-format/data/loanwords.tsv: common English words with coarse
phonetic keys, used to turn Devanagari-spelled English (ऑफिस, डॉक्टर) back into
English spelling (office, doctor) in Roman-Hinglish output.

Sources (pinned and verified, both permissively licensed):
  - CMUdict (BSD-2-Clause), pronunciations
  - SCOWL 2020.12.07 (permissive, see its Copyright file), word commonness tiers 10/20/35

Key alphabet (must match bolo_format::loanwords on the Devanagari side):
  consonants  p b t d k g c(ch) j(j/z/zh) f(f/ph) w(v/w) s(s/sh) h m n l r y
  vowels      A(a/aa/schwa) E(e/ai/ae) I(i/ee) O(o/au/aw-ish) U(u/oo)
Aspiration and dental/retroflex are merged because Hindi spellings of English words
vary there.

Usage: python -I scripts/build_loanword_lexicon.py
"""
import hashlib, io, pathlib, re, sys, tarfile, urllib.request

CMUDICT_COMMIT = "74790861f652b15e4ac49015a90074ad62a27690"
CMUDICT_URL = f"https://raw.githubusercontent.com/cmusphinx/cmudict/{CMUDICT_COMMIT}/cmudict.dict"
SCOWL_URL = "https://downloads.sourceforge.net/project/wordlist/SCOWL/2020.12.07/scowl-2020.12.07.tar.gz"
SCOWL_SHA256 = "5587667caa20c4891390c2d42dbb4d5c4c3f41bee77af1457ece3ba23fb859cc"
# Lower tier = more common; used as tie-break. 40/50 add tech vocabulary (deploy, screenshot).
TIERS = (10, 20, 35, 40, 50)
CAPITALIZED_OK = {
    "monday", "tuesday", "wednesday", "thursday", "friday", "saturday", "sunday",
    "january", "february", "march", "april", "may", "june", "july", "august",
    "september", "october", "november", "december",
}
OUT = pathlib.Path(__file__).resolve().parent.parent / "crates" / "bolo-format" / "data" / "loanwords.tsv"

CONS = {
    "P": "p", "B": "b", "T": "t", "D": "d", "K": "k", "G": "g", "CH": "c", "JH": "j",
    "F": "f", "V": "w", "W": "w", "TH": "t", "DH": "d", "S": "s", "Z": "j", "SH": "s",
    "ZH": "j", "HH": "h", "M": "m", "N": "n", "NG": "ng", "L": "l", "R": "r", "Y": "y",
}
VOWELS = {
    "AH": ["A"], "AE": ["E"], "AO": ["O"], "OW": ["O"], "EH": ["E"], "EY": ["E"],
    "IH": ["I"], "IY": ["I"], "UH": ["U"], "UW": ["U"], "ER": ["Ar"], "AW": ["AU"],
    "AY": ["AI"], "OY": ["OI"],
}


def fetch(url):
    req = urllib.request.Request(url, headers={"User-Agent": "bolo-build"})
    return urllib.request.urlopen(req, timeout=120).read()


def scowl_words():
    data = fetch(SCOWL_URL)
    got = hashlib.sha256(data).hexdigest()
    if got != SCOWL_SHA256:
        sys.exit(f"SCOWL sha256 mismatch: {got}")
    tiers = {}
    with tarfile.open(fileobj=io.BytesIO(data)) as tar:
        names = set(tar.getnames())
        for tier in TIERS:
            # british-words: Indian English follows British spelling (centre, colour).
            for kind in ("words", "british-words", "upper"):
                name = f"scowl-2020.12.07/final/{kind if kind == 'british-words' else 'english-' + kind}.{tier}"
                if name not in names:  # e.g. there is no english-upper.20
                    continue
                for line in tar.extractfile(name).read().decode("latin-1").splitlines():
                    w = line.strip()
                    if not re.fullmatch(r"[A-Za-z]+", w):
                        continue
                    # Capitalized words are mostly names/places (Malta, Milton) that collide
                    # with Hindi; keep only days and months.
                    if not w.islower() and w.lower() not in CAPITALIZED_OK:
                        continue
                    prev = tiers.get(w.lower())
                    # Keep the most common tier; prefer the lowercase spelling when a word
                    # is both common and capitalized (doctor / Doctor).
                    tier_best = min(tier, prev[0]) if prev else tier
                    display = w if (prev is None or (w.islower() and not prev[1].islower())) else prev[1]
                    tiers[w.lower()] = (tier_best, display)
    return tiers  # lower -> (tier, display form)


def aa_variants(word):
    """AA is written ॉ/ो when the English spelling has an 'o' (doctor, project), ा
    otherwise (start, father)."""
    has_o, has_a = "o" in word, "a" in word
    if has_o and not has_a:
        return ["O"]
    if has_a and not has_o:
        return ["A"]
    return ["A", "O"]


def keys_for(word, phones):
    keys = [""]
    bases = [re.sub(r"\d", "", p) for p in phones]
    for i, base in enumerate(bases):
        nxt = bases[i + 1] if i + 1 < len(bases) else None
        if base in CONS:
            opts = [CONS[base]]
        elif base == "AA":
            opts = aa_variants(word)
        elif base in ("EH", "EY") and nxt == "R":
            # share/care/air: Hindi writes -एयर (शेयर) as well as -एर.
            opts = ["E", "EyA"]
        elif base in VOWELS:
            opts = VOWELS[base]
        else:
            return []
        keys = [k + o for k in keys for o in opts]
    return keys


def main():
    common = scowl_words()
    seen = set()
    rows = []
    for line in fetch(CMUDICT_URL).decode("utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        head, *phones = line.split()
        word = re.sub(r"\(\d+\)$", "", head)
        if word not in common:
            continue
        tier, display = common[word]
        for key in keys_for(word, phones):
            skeleton = "".join(c for c in key if c.islower())
            # Short words collide with Hindi (में/main, है/hi): require ≥2 consonants
            # and ≥3 phonemes.
            if len(skeleton) < 2 or len(key) < 3:
                continue
            if (key, display) in seen:
                continue
            seen.add((key, display))
            rows.append((skeleton, key, display, tier))
    rows.sort()
    OUT.parent.mkdir(parents=True, exist_ok=True)
    with open(OUT, "w", encoding="utf-8", newline="\n") as f:
        f.write("# Generated by scripts/build_loanword_lexicon.py — do not edit.\n")
        f.write(f"# CMUdict {CMUDICT_COMMIT} (BSD-2-Clause, Carnegie Mellon University)\n")
        f.write("# SCOWL 2020.12.07 (Copyright 2000-2018 Kevin Atkinson; permissive, see data/README.md)\n")
        f.write("# skeleton<TAB>key<TAB>word<TAB>tier\n")
        for r in rows:
            f.write("\t".join(map(str, r)) + "\n")
    print(f"{len(rows)} keys for {len({r[2] for r in rows})} words -> {OUT} ({OUT.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
