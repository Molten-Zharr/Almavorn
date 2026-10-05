# Almavorn

[English](README.md) | [Русский](README.ru.md)

Stack: Rust, Ratatui, Tokio, Rodio, SQLite, Crossterm, egui, eframe, egui_ratatui.

Releases: TUI-only and GUITUI.

Build and run:

```sh
cargo run --release
cargo run --release --no-default-features --features tui --bin almavorn-tui
```

SQLite is compiled and embedded in the executables through rusqlite. No
DuckDB runtime library is required. The active database is `almavorn.sqlite`.
On first launch, an existing `almavorn.duckdb` is imported using an installed
DuckDB CLI; otherwise `almavorn.sqlite3` is imported if present. Both original
files are preserved. If DuckDB import fails, the application stops without
falling back to an older library. DuckDB CLI is only needed for this one-time
import. The DuckDB application is preserved in the `archive/duckdb` branch.

Both interfaces share application state and Ratatui rendering:

- `src/app.rs`: shared state, initialization, and the update loop.
- `src/app/`: actions and input, library and undo history, background database
  operations and import,
  playback, dialogs, and command-line options.
- `src/ui.rs`: main-screen layout.
- `src/ui/`: library and player panels, themes, shared widgets, and dialogs.
- `src/store.rs`: SQLite storage with WAL, serialized background writes, and legacy migration.
- `src/preferences.rs`: profiles, palettes, theme presets, and palette exchange.

## Settings

Open Settings from the application menu. Sections appear on the left,
parameters and descriptions on the right, and keyboard hints and buttons
at the bottom. Pages cover general settings, profiles, themes, palettes,
typography and geometry, and shortcuts. Open a full parameter description
with `F1`, the `?` button, or a double click on its row.

- `Tab` moves focus between the menu and parameters; `↑`/`↓` select a row,
  `←`/`→` change a value, `Enter` runs an action, and `Esc` goes back.
- Use the mouse to select pages and rows, change values with buttons,
  scroll lists, and enter text through the on-screen keyboard.
- In catalogs, `Ins` creates a copy, `F3` renames, and `Del` requests deletion.
  On the palettes page, `Ctrl+I` imports and `Ctrl+E` exports JSON.

Changes save automatically to the active profile. Each profile stores the
language, volume, layout, key bindings, and appearance. Switching profiles
saves the previous preferences and preserves the music library. Palette and
preset catalogs are shared: editing a palette updates every theme and profile
that uses it.

**Molten-Zharr** is the first theme and palette, containing all 16 color roles
from the organization's palette. Its source is bundled in
[`assets/brand/molten-zharr.json`](assets/brand/molten-zharr.json), with role
descriptions in [`assets/brand/PALETTE.md`](assets/brand/PALETTE.md).
Classic Amber, Classic Violet, and Light are also available. Order and Chaos
can use different palettes. Presets capture the palette, font, size, borders,
and corners; create, update, rename, and delete them from the themes page.
Existing configuration colors migrate into Legacy palettes.

Choose Fira Code Bold, Fira Code, or DejaVu Sans Mono at 10–32 pixels, with
no borders, single, double, or thick borders. Rounded corners apply to single
borders. Font and size changes take effect immediately in GUITUI; the terminal
controls them in TUI. Bundled font licenses are in `assets/fonts/`.

Copy, rename, edit each HEX color, and delete palettes. Import accepts Almavorn
JSON (`almavorn.palette`, version 1) and Molten-Zharr palette JSON (version 2),
adding an independent copy. Export writes a new file at the entered path and
preserves existing files. Removing a palette redirects its references to a
remaining palette. Each catalog retains at least one entry.

Render every settings page without a window or audio device:

```sh
cargo run --release --example settings_preview
```

PPM images are written to `target/settings-preview/`.

Regression tests cover playlist editing, undo/redo, background import, profiles
and palettes, settings migration, input, and rendering. They do not require
an audio output device:

```sh
cargo test --release --all-features
cargo test --release --no-default-features --features tui
```
