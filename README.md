# Almavorn

[English](README.md) | [Русский](README.ru.md)

A music player for Linux: keep your collection organized and your mixes flexible.

- Keep your favorite playlists protected from accidental changes.
- Create new mixes while preserving your original playlists.
- Edit song titles, tags, and cover artwork together.
- Shape your sound with equalizer, tempo, and pitch.
- Choose themes, keyboard shortcuts, and your panel layout.
- Listen in a desktop window or your terminal.

## Install

Choose your interface for Linux x86_64:

- GUITUI, a desktop window: [Flatpak](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-guitui-linux-x86_64.flatpak) · [AppImage](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-guitui-linux-x86_64.AppImage) · [tar.gz](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-guitui-linux-x86_64.tar.gz).
- TUI, your terminal: [Flatpak](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-tui-linux-x86_64.flatpak) · [AppImage](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-tui-linux-x86_64.AppImage) · [tar.gz](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-tui-linux-x86_64.tar.gz).

- Flatpak: open with your system's software installer.
- AppImage: allow execution in file properties, then launch.
- tar.gz: extract, then run "python3 install.py" inside.

Run the TUI AppImage from a terminal. For TUI Flatpak, use:

```sh
flatpak run io.github.MoltenZharr.Almavorn.TUI
```

Flatpak needs an internet connection for its first installation.
Its library and settings are separate from the other formats.

## Build and install

Ubuntu, Debian, Arch Linux, and Artix. For a desktop window:

```sh
curl -fsSL https://raw.githubusercontent.com/Molten-Zharr/Almavorn/main/install.sh | bash -s -- --variant guitui
```

For the terminal only:

```sh
curl -fsSL https://raw.githubusercontent.com/Molten-Zharr/Almavorn/main/install.sh | bash -s -- --variant tui
```

The command installs dependencies and your chosen interface. It may ask for
your administrator password. To update, close Almavorn if it is running
and repeat your installation command.

Open Almavorn from your application menu after installation.
For TUI, run "~/.local/bin/almavorn-tui".

[Ready-to-use download](https://github.com/Molten-Zharr/Almavorn/releases/latest) ·
[Keyboard shortcuts](docs/GLOBAL_SHORTCUTS.md) ·
[Developer guide](docs/RELEASE.md) · [MIT](LICENSE)
