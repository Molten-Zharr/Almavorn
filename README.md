# Almavorn

[English](README.md) | [Русский](README.ru.md)

Stack: Rust, Ratatui, Tokio, Rodio, DuckDB, Crossterm, egui, eframe, egui_ratatui.

Releases: TUI-only and GUITUI.

Build and run:

```sh
cargo run --release
cargo run --release --no-default-features --features tui --bin almavorn-tui
```

The build downloads the official prebuilt DuckDB library and places it next to
the executables.

Both interfaces share application state and Ratatui rendering:

- `src/app.rs`: shared state, initialization, and the update loop.
- `src/app/`: actions and input, library and undo history, background database
  operations and import,
  playback, dialogs, and command-line options.
- `src/ui.rs`: main-screen layout.
- `src/ui/`: library and player panels, themes, shared widgets, and dialogs.
- `src/store.rs`: DuckDB storage and migration from an existing SQLite database.

Regression tests cover playlist editing, undo/redo, background import, input,
and rendering. They do not require an audio output device:

```sh
cargo test --release --all-features
```
