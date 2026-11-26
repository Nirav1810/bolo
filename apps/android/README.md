# apps/android (M5)

Kotlin + Jetpack Compose voice keyboard (`InputMethodService`) loading `libbolo_ffi.so` (UniFFI). arm64-v8a, minSdk 28. Not started yet.

## Design

Mockups: the "Bolo Android UI" design canvas, https://claude.ai/artifact/2Zri3YLzaTiqDqpeYjCJfZ (private to the user). The decisions and their reasons are in `.scratch/m5-android/issues/03-android-mockups.md`. They're waiting for the user's review, which gates tickets 06 and 07.

The design is the desktop's (`apps/desktop/src/theme.css`) in Material 3 form. Dynamic colour is off, so the primary is always Bolo's indigo, and red (`live`) means only "the mic is recording". Light and dark follow the system setting.

### Keyboard panel

- **Height:** 264 dp above the system navigation bar, the same as Gboard's Normal height. That's 8 top padding, a 48 strip (the pill), a 136 mic area (88 mic, 56 cancel), 48 keys plus an 8 gap, and 16 bottom padding.
- **Keys:** switch keyboard 56 wide, space filling the rest, backspace 56, the field's enter action 72 (primary). Keys have a 10 dp radius with a 6 dp gap.
- **Pill:** 40 dp tall, fully round, at most 248 dp wide. It holds an 8 dp live dot and 16 waveform bars (3 dp wide, 2 dp gap, 3–24 dp tall, in primaryText).

### Colour (Compose `ColorScheme` + `BoloColors` extras)

| Role | Light | Dark |
| --- | --- | --- |
| primary | `#1B4ED8` | `#2E64EA` |
| onPrimary | `#FFFFFF` | `#FFFFFF` |
| primaryText (extra) | `#1B4ED8` | `#87AFFF` |
| primaryContainer | `#EAEFFB` | `#15213E` |
| background | `#FCFCFC` | `#000000` |
| surfaceContainerLowest (cards) | `#FFFFFF` | `#0E0E0E` |
| surfaceContainer | `#F4F4F5` | `#1B1B1B` |
| onSurface | `#18181B` | `#F2F2F2` |
| onSurfaceVariant | `#65656D` | `#A3A3A3` |
| outline | `#8A8A94` | `#6B6B6B` |
| outlineVariant | `#E4E4E7` | `#222222` |
| error | `#B91C1C` | `#F87171` |
| live (extra) | `#E5484D` | `#FF5A5F` |
| success (extra) | `#047857` | `#34D399` |
| warning (extra) | `#A84D08` | `#FBBF24` |
| keyboard (extra) | `#EEEEF1` | `#121212` |
| key (extra) | `#FFFFFF` | `#2C2C2C` |
| function key (extra) | `#DCDCE1` | `#1F1F1F` |

### Type (sp, system font)

headlineSmall 24/32 500 · titleLarge 22/28 400 · titleMedium 16/24 500 · bodyLarge 16/24 400 · bodyMedium 14/20 400 · labelLarge 14/20 500 · labelMedium 12/16 500. Nothing is smaller than 12 sp. Devanagari falls back to Noto Sans Devanagari.

### Shape, spacing and motion (dp)

- **Radius:** keys 10, fields 8, cards 12, dialogs 28. Buttons, chips and the pill are fully round.
- **Spacing:** a 4 dp grid with a 16 dp screen margin. List rows are 56 or 72.
- **Touch targets:** at least 48. Buttons are 40 tall inside a 48 target.
- **Icons:** Lucide vector drawables, 24 dp with a 2 dp stroke.
- **Motion:** `cubic-bezier(.23,1,.32,1)`: 150 ms press, 200 ms enter, 140 ms exit, and the polish tint fades over 600 ms. "Remove animations" keeps fades and drops movement.

### Navigation

The launcher icon opens a home screen with a status card and one grouped list: General, Models, Cloud keys, Dictionary and snippets, Apps, History and About. Each opens a page with a back arrow (list-detail). There's no bottom bar.
