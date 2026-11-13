// The settings window's pages: each one's name and its one line of explanation, written
// once. Anything that names a page (a button, a sentence) takes the label from here, so
// the nav and the prose can't drift apart. The id is also the URL hash Rust opens.
export const PAGES = {
  general: { label: "General", blurb: "How you start a dictation, and the language Bolo listens for." },
  engines: { label: "Engines and keys", blurb: "The speech engines that turn your voice into text, and the API keys for the cloud ones." },
  polish: { label: "Polish", blurb: "Claude tidies each dictation after it is inserted, then swaps the polished text in." },
  apps: { label: "Apps", blurb: "How text is inserted into each app: the format, and polish and its tone." },
  dictionary: { label: "Dictionary and snippets", blurb: "Words and names Bolo should spell your way, and phrases it expands to saved text." },
  history: { label: "History", blurb: "Your dictations, kept as text on this PC." },
  usage: { label: "Usage", blurb: "What your cloud engines used, and what it cost." },
  about: { label: "About", blurb: "Version, updates and where Bolo keeps its files." },
} as const;

export type Page = keyof typeof PAGES;

export const PAGE_IDS = Object.keys(PAGES) as Page[];
