# Almavorn

[English](README.md) | [Русский](README.ru.md)

Стек: Rust, Ratatui, Tokio, Rodio, SQLite, Crossterm, egui, eframe, egui_ratatui.

Версии: TUI-only и GUITUI.

Сборка и запуск:

```sh
cargo run --release
cargo run --release --no-default-features --features tui --bin almavorn-tui
```

Для сборки нужны компилятор C++, libclang со стандартными заголовками и файлы разработки ALSA.

[Скачать](https://github.com/Molten-Zharr/Almavorn/releases) · [Установка и подготовка выпуска](docs/RELEASE.md) · [Лицензия MIT](LICENSE).
