# Almavorn

[English](README.md) | [Русский](README.ru.md)

Музыкальный плеер для Linux: порядок в коллекции, свобода в подборках.

- Храните любимые плейлисты с защитой от случайных изменений.
- Собирайте новые подборки, сохраняя ваши исходные плейлисты.
- Редактируйте названия, теги и обложки своих композиций.
- Настраивайте звучание эквалайзером, темпом и высотой тона.
- Выбирайте темы, горячие клавиши и расположение панелей.
- Слушайте музыку в окне или прямо в терминале.

## Установка

Выберите интерфейс для Linux x86_64:

- GUITUI, отдельное окно: [Flatpak](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-guitui-linux-x86_64.flatpak) · [AppImage](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-guitui-linux-x86_64.AppImage) · [tar.gz](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-guitui-linux-x86_64.tar.gz).
- TUI, работа в терминале: [Flatpak](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-tui-linux-x86_64.flatpak) · [AppImage](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-tui-linux-x86_64.AppImage) · [tar.gz](https://github.com/Molten-Zharr/Almavorn/releases/download/v0.1.4/almavorn-0.1.4-tui-linux-x86_64.tar.gz).

- Flatpak: откройте файл через системный установщик приложений.
- AppImage: разрешите запуск в свойствах файла, затем откройте.
- tar.gz: распакуйте и выполните внутри "python3 install.py".

TUI AppImage запускайте из терминала. Для TUI Flatpak:

```sh
flatpak run io.github.MoltenZharr.Almavorn.TUI
```

Для первой установки Flatpak нужен интернет.
Его библиотека и настройки хранятся отдельно от других форматов.

## Собрать и установить

Ubuntu, Debian, Arch Linux и Artix. Для отдельного окна:

```sh
curl -fsSL https://raw.githubusercontent.com/Molten-Zharr/Almavorn/main/install.sh | bash -s -- --variant guitui
```

Только для терминала:

```sh
curl -fsSL https://raw.githubusercontent.com/Molten-Zharr/Almavorn/main/install.sh | bash -s -- --variant tui
```

Команда подготовит зависимости и установит выбранный интерфейс.
При необходимости запросит пароль администратора. Для обновления закройте
Almavorn, если он запущен, и повторите свою команду установки.

После установки откройте Almavorn через меню приложений.
Для TUI выполните "~/.local/bin/almavorn-tui".

[Готовый выпуск без сборки](https://github.com/Molten-Zharr/Almavorn/releases/latest) ·
[Горячие клавиши](docs/GLOBAL_SHORTCUTS.md) ·
[Для разработчиков](docs/RELEASE.md) · [MIT](LICENSE)
