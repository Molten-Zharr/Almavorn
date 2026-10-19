# Almavorn

[English](README.md) | [Русский](README.ru.md)

Stack: Rust, Ratatui, Tokio, Rodio, SQLite, Crossterm, egui, eframe, egui_ratatui.

[Global shortcuts](docs/GLOBAL_SHORTCUTS.md): control the running player through compositor commands.

Releases: TUI-only and GUITUI.

Build and run:

```sh
cargo run --release
cargo run --release --no-default-features --features tui --bin almavorn-tui
```

Building requires a C++ compiler, libclang with standard headers, and ALSA development files.

[Downloads](https://github.com/Molten-Zharr/Almavorn/releases) · [Installation and release guide](docs/RELEASE.md) · [MIT license](LICENSE).
