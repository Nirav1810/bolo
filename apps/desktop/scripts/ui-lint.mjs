// Checks the mechanical UI rules of src/lib/ui/README.md. Part of `npm run check`.
import { readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative, sep } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(fileURLToPath(new URL(".", import.meta.url)), "..");
const src = join(root, "src");

function files(dir) {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    return statSync(path).isDirectory() ? files(path) : [path];
  });
}

const COLOUR = "bg|text|border|ring|outline|decoration|fill|stroke|shadow|divide|from|via|to|accent|caret|placeholder";

/** [what is wrong, pattern, where: "all" | "pages" (not lib/ui) | "style" (a <style> block)] */
const RULES = [
  ["`dark:` variant: light and dark differ only in theme.css", /\bdark:/g, "all"],
  ["alpha on a colour: add a token to theme.css", new RegExp(`\\b(?:${COLOUR})-[a-z][a-z-]*\\/\\d+`, "g"), "all"],
  ["arbitrary text size: use the five-step scale", /\btext-\[/g, "all"],
  ["text size outside the scale (display, title, heading, sm, xs)", /\btext-(?:2xs|base|lg|xl|[2-9]xl)\b/g, "all"],
  ["`leading-*` or `tracking-*`: the type scale sets both", /(?<![\w-])(?:leading|tracking)-[\w[(]/g, "all"],
  ["`!` override: fix the component instead", /(?<![\w-])[a-z][\w-]*-[\w.[\]/]+!(?=[\s"'`])/g, "all"],
  ["`duration-<n>`: use the default, or duration-(--duration-base)", /\bduration-\d/g, "all"],
  ["raw colour: use a token from theme.css", /(?<![&\w])#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?|oklch|color-mix)\(/g, "all"],
  ["negative margin: use a SettingsSection footer or a gap", /(?<![\w-])-m[trblxy]?-\d/g, "all"],
  ["raw <button>: use Button", /<button\b/g, "pages"],
  ["raw <select>: use Select", /<select\b/g, "pages"],
  ["raw <dialog>: use ConfirmDialog", /<dialog\b/g, "pages"],
  ["removed class: use `field`, `font-mono`, Select or Progress", /class="[^"]*(?<![\w-])(?:input|select|textarea|progress|mono)(?![\w-])[^"]*"/g, "pages"],
  ["hand-written card: use Card or SettingsSection", /rounded-xl border\b[^"]*\bbg-card\b/g, "pages"],
  ["straight apostrophe in text: use ’", /[A-Za-z]'(?:t|s|re|ll|ve|d|m)\b/g, "text"],
  ["`&` in text: write “and”", /[A-Za-z] &(?:amp;)? [A-Za-z]/g, "text"],
  ["font-size in px: use var(--text-xs) or var(--text-sm)", /font-size:\s*[\d.]+px/g, "style"],
];

const problems = [];
for (const file of files(src)) {
  if (!file.endsWith(".svelte")) continue;
  const name = relative(root, file).split(sep).join("/");
  const inUi = name.startsWith("src/lib/ui/");
  let inStyle = false;
  let inComment = false;
  readFileSync(file, "utf8")
    .split("\n")
    .forEach((line, i) => {
      if (line.includes("<style")) inStyle = true;
      const trimmed = line.trim();
      if (trimmed.startsWith("<!--") || trimmed.startsWith("/*")) inComment = true;
      const comment = inComment || trimmed.startsWith("//") || trimmed.startsWith("*");
      if (trimmed.endsWith("-->") || trimmed.endsWith("*/")) inComment = false;
      for (const [message, pattern, where] of RULES) {
        if (where === "pages" && inUi) continue;
        if (where === "style" && !inStyle) continue;
        if (where === "text" && (comment || inStyle)) continue;
        if (comment && where !== "text") continue;
        const code = where === "text" ? line.split(" // ")[0] : line;
        for (const m of code.matchAll(pattern)) problems.push(`${name}:${i + 1}  ${message}  →  ${m[0].slice(0, 60)}`);
      }
      if (line.includes("</style>")) inStyle = false;
    });
}

// ui.css holds no colours of its own either.
readFileSync(join(src, "ui.css"), "utf8")
  .split("\n")
  .forEach((line, i) => {
    for (const m of line.matchAll(/(?<![&\w])#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?|oklch|color-mix)\(/g)) {
      problems.push(`src/ui.css:${i + 1}  raw colour: use a token from theme.css  →  ${m[0]}`);
    }
  });

if (problems.length) {
  console.error(problems.join("\n"));
  console.error(`\nui-lint: ${problems.length} problem${problems.length === 1 ? "" : "s"}. The rules are in src/lib/ui/README.md.`);
  process.exit(1);
}
console.log("ui-lint: ok");
