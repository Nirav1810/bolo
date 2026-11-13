# Bolo desktop UI rules

One way to do each thing. Pages (`src/settings`, `src/onboarding`, `src/Debug.svelte`, `src/Pill.svelte`) are built only from the components here, the classes in `src/ui.css` and the tokens in `src/theme.css`. `npm run check` runs `scripts/ui-lint.mjs`, which fails on the mechanical rules below.

If a page needs a look that isn't here, add a token or a component. Don't spell it in the page.

## Colour

- Every colour is a token from `theme.css` (`bg-card`, `text-muted-foreground`, `border-divider`, `bg-warning-tint`). No `dark:` variant, no `/NN` alpha on a colour, no hex, `rgb()` or `color-mix()` outside `theme.css` (`ui.css` included).
- Text is `text-foreground` or `text-muted-foreground`. Status text is `text-{success,warning,destructive}-foreground`, and only inside `Status`, `Alert` and `Badge`.
- Fills: `bg-card` (a card), `bg-inset` (a panel set into a card), `bg-muted` (a quiet fill), `bg-accent` (hover), `bg-track` (a trough).
- Edges: `border-border` (a card), `border-divider` (hairlines inside one), `border-input` (a button), `border-field-edge` (a field).
- Primary marks the chosen thing: the on switch, the current nav icon, a "Primary" badge, a primary button. `live` red means only "the mic is recording". Nightly purple means only "this is the Nightly build".
- State never relies on colour alone: warning, error and success always come with their icon (`Status`, `Alert`, `IconTile` do this).

## Type

Five sizes, nothing else, nothing under 12 px:

| Class | Size | Use |
| --- | --- | --- |
| `text-display` | 24 | an onboarding step's headline (`PageHeader size="hero"`) |
| `text-title` | 20 | a page's title (`PageHeader`) |
| `text-heading` | 16 | a section's title, a dialog's title, a big number |
| `text-sm` | 14 | everything by default: row titles, buttons, fields, body |
| `text-xs` | 12 | secondary: descriptions, captions, badges, status lines |

- The top three carry their own weight and tracking. Elsewhere weight is `font-medium` (titles, labels, buttons) or normal. `font-semibold` only on a number or name that is the point of its card.
- No `text-[..]`, `leading-*` or `tracking-*` in a page. Numbers that change or line up get `tabular-nums`. Code, ids, paths and keys get `font-mono` at the size of the text around them.

## Size, shape, space

- Controls are 32 px tall, or 28 px (`sm`) inside a card: buttons, fields, `Select` and `Segmented` alike. 32 px is for a page's own bar (History's search, Usage's period), a dialog and the onboarding footer. Radius: 12 px cards (`rounded-xl`), 8 px controls and tiles (`rounded-lg`), 6 px for what sits inside a control (`rounded-md`).
- Icons are lucide, `size-4`; `size-3.5` beside 12 px text. Always `aria-hidden="true"` next to a label.
- A page is a column with `gap-8` between sections. A section is title, card, footer with `gap-2.5`. Inside a card, rows are `px-4 py-3` (that is `SettingsRow`), free blocks `px-4 py-3.5`. Gaps between controls: `gap-1.5` in a row's control slot, `gap-2` in a form line, `gap-3` between stacked fields.
- No negative margins, no `!` overrides, no `ml-11`-style alignment by hand. `SettingsRow` lines up what sits under a row.

## Components

| Need | Use |
| --- | --- |
| A page's title and blurb | `PageHeader` |
| A titled group of rows | `SettingsSection` (`title`, `description`, `action`, `footer`) |
| A row in a card | `SettingsRow` (`leading`, `title`, `badges`, `description`, `status`, `control`, children). Every row in a card is one. |
| A card that isn't a section | `Card` (`divided`, `as`) |
| A panel set into a card (add/edit form, more settings) | `Inset` |
| The square an icon sits in | `IconTile` (`tone`, `size`) |
| A button | `Button`. Never a raw `<button>`. |
| Delete one thing | `ConfirmButton` |
| Delete or reset everything | `ConfirmDialog` |
| On/off | `Switch` |
| One of 2–4 short choices | `Segmented` |
| One of a list | `Select`. Never a raw `<select>`. |
| A text field | `<input class="field">`, plus `field-sm` inside a card. `<textarea class="field">`. |
| A labelled field in a form | `Field` |
| Paste an API key | `KeyField` |
| A fact about a row (Saved, Primary, Recommended) | `Badge` in the row's `badges` |
| How one thing is doing, or how an action on it went | `Status` (a row's `status` prop, or under the control) |
| A block message | `Alert` |
| An empty list or page | `Empty`, inside a card |
| A page's cards while loading | `Skeleton` |
| A download or other progress | `Progress` |
| The mic's level | `LevelMeter` |
| A key on the keyboard | `Kbd` |
| A word in a set | `Chip` |
| A link in a sentence | `TextLink` |
| The app's icon, name and build | `AppMark` |

### Buttons

- Variant by job: `primary` = a form's submit or the one forward action; `outline` = a row's own action, and the first action of an Alert or Empty; `ghost` = the lesser action beside it, Cancel, a section header's action, a link out; `quiet` = an icon button. `danger` and `quiet-danger` only inside `ConfirmButton` and `ConfirmDialog`.
- Size by place: `sm` inside a card (rows, section headers, Alerts, Empties, Insets); `default` in a page's own bar, a dialog, the onboarding footer, and beside a 32 px field. Icon buttons are `icon-sm` (in a card) or `icon`; `icon-xs` only inside another control.
- An icon button has an `aria-label` written "Verb the specific thing" ("Remove Kubernetes", "Copy this dictation"). `Button` shows it as the tooltip, so never pass `title`.
- A leading icon is for an action, a trailing one for going somewhere (`ExternalLink`, `ArrowRight`). Work a button started shows on that button: `loading`.
- A label ends with "…" only when the press opens a picker, a field or a confirmation instead of finishing.
- Row actions are always visible. Nothing appears on hover only.

### Rows and tiles

- A row gets an `IconTile` when it stands for a thing (an engine, a model, a key, an app, a snippet, a folder). A row that is a setting has none. Never mix the two in one section.
- The tile takes a tone when the row is in that state (`primary` for the primary engine, `success` for a saved key, `warning`/`error` for a problem).

### Destructive actions

- **Delete** erases what the user would have to retype, re-enter or download again (a dictation, a snippet, a saved key, a model). Trash icon, always `ConfirmButton`.
- **Remove** takes an entry out of a list that one click puts back (an engine in the chain, a dictionary word). `X` icon, `quiet`, never confirmed.
- **Reset** puts values back to defaults or zero. "Clear" is not used.
- Everything at once ("Delete all…", "Reset usage…") opens a `ConfirmDialog` titled with the question.

### Feedback

- A failed operation is an `Alert variant="error"` titled "Couldn’t <verb> <object>" with the detail through `sentence()` as its body. It goes first under the page header, or in the section's `footer` when it is about that section.
- A message about one row or control is a `Status`. Never grey text for a failure, never a hand-written coloured line.
- A change that shows in its own control (a switch, a choice) gets no message.
- Loading: `Skeleton` for a page, `loading` on the button that started the work, `Status tone="busy"` for a check.
- After adding, removing or closing something, put the focus where the user goes next (the field, the button that opened the panel). Don't let it fall to the page.

## Words

- "and", never "&". Sentence case everywhere. A sentence ends with a period wherever it appears; titles, labels, buttons and badges don't.
- Curly apostrophes and quotes (’ “ ”) in everything the user reads. "·" between facts, "–" for ranges, "›" for paths.
- A page is named by its label from `lib/pages.ts` ("Open {PAGES.engines.label}"). Never retype it.
- One word per thing: push-to-talk key, API key, admin key (never bare "key" outside a row already titled for it); engine chain, primary, fallback; engine (what transcribes) and model (what is downloaded); microphone (never "mic"); Windows default; this PC; dictation (an item) and History (the page); instant text, polished text, swap in. Text is *inserted*; the user *holds*, *dictates* and *lets go*.
- One verb per action: Make primary, Add as fallback, Save (a key), Add <thing>, New <thing>…, Continue (onboarding), Cancel (stop), Try again (after a failure), Check (a key, the microphone), Show/Hide settings.
- "Claude" is the model that polishes. "Anthropic" is the provider, the API key and the usage card. Names of providers, tones, languages and formats come from the maps in `lib/api.ts`.
- "Bolo" in prose about what it does. `variant(version).product` where the sentence is about this installed app (window title, About, updates, the pill).
- Placeholders: "<Noun>, like <example>…", or "Paste <thing>…" / "Search <things>…". Empty states: "No <things> yet" and one sentence.
- Counted nouns go through `plural()`. Units: a number, a space, the unit (ms, s, min, MB, GB).

## Motion

- Transitions use the default (150 ms, `--ease-out-ui`). No `duration-<n>` classes; when something needs longer, `duration-(--duration-base)`.
- `.enter` for a page or panel arriving, `.pop` for a popup. No ease-in. Reduced motion keeps fades and drops movement (handled in `theme.css` and `ui.css`).

## Focus and disabled

- `focus-ring` on anything pressable (2 px ring, 2 px out, keyboard only). Fields take the ring colour on their edge. Nothing else draws focus.
- Disabled is `disabled-dim` (opacity 0.5). Nothing else.
