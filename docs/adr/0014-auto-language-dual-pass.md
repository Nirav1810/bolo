# Auto language mode: decode English and Hindi in parallel, choose once per dictation

Nemotron's built-in `auto` language misdetects English as Hindi (English WER 8.0% → 17.2% on our clips), while forcing the wrong language is far worse (English decoded as Hindi: 67.6%). So in the default **Auto** language mode we run two streams on the one loaded model, one forced to English and one to Hindi, and at key release choose a single result for the whole dictation by how many Hindi function words the Hindi pass contains. This costs a second decode's CPU during speech but adds almost nothing after release, which is the latency the user feels (ADR-012). Users who only speak one language can set the language mode to **English** or **Hinglish** to skip the second pass.

Details:

- **Ambiguous dictations** ("ok", "send it") reuse the previous dictation's language; English when there is no history.
- **Live preview** shows whichever pass is currently ahead, so it may flip mid-dictation; only the final choice is inserted.
- **Shipping bar**: on the 40-clip set, Auto must reach English WER ≤ 8.5% and Hinglish WER ≤ 30% with release→text p50 ≤ 180 ms. If it misses, Auto ships opt-in and the default stays manual.
- **Scope**: low-tier Android (Moonshine / Whisper-Hinglish models) does not offer Auto; the language is picked at onboarding. Cloud engines in Auto send their own multilingual mode (Deepgram `language=multi`) instead of Bolo's detector.

## Considered Options

- **Nemotron `auto`**: rejected on accuracy (above).
- **Manual setting only**: rejected as the default; switching a setting mid-conversation is the friction Bolo exists to remove.
- **Per-sentence or per-word detection**: rejected; it needs sentence splitting inside the instant path and makes output switch style mid-paragraph. Mixed speech is already handled by the Hinglish path, which writes English words in Roman script.
