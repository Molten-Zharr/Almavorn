# Сборка и выпуск Almavorn

## Установка из исходников

Нужны Rust, C++, libclang и заголовки ALSA. В чистой копии репозитория:

```sh
python3 scripts/package-release.py --install --variant guitui
```

Для TUI замените guitui на tui. Плеер появится в меню приложений.

## Подготовка выпуска

Обновите версию в Cargo.toml и Cargo.lock, добавьте описание в releases/vX.Y.Z.md и закоммитьте изменения.

Для локальных архивов:

```sh
python3 scripts/package-release.py --variant guitui
python3 scripts/package-release.py --variant tui
```

Файлы появятся в target/dist/ВЕРСИЯ/ИНТЕРФЕЙС.

Для публикации замените X.Y.Z на номер версии:

```sh
git tag -a vX.Y.Z -m "Almavorn X.Y.Z"
git push origin refs/tags/vX.Y.Z
```

GitHub Actions соберет оба интерфейса и опубликует пакеты в [Releases](https://github.com/Molten-Zharr/Almavorn/releases). Проверьте завершение сборки и наличие файлов.

## Лицензии и пересборка SoundTouch

Almavorn — MIT, SoundTouch — LGPL-2.1, Symphonia — MPL-2.0. Полный список лицензий входит в пакет: THIRD_PARTY_NOTICES.md и share/licenses.

В share/source лежат исходники плеера и dependencies.tar.gz с исходниками зависимостей. Для замены статически включенного SoundTouch распакуйте оба архива в одну папку и добавьте в Cargo.toml плеера:

```toml
[patch.crates-io]
soundtouch = { path = "../soundtouch-0.5.4" }
soundtouch-ffi = { path = "../soundtouch-ffi-0.4.1" }
```

Измените SoundTouch в soundtouch-ffi-0.4.1/soundtouch-2_3_2 и соберите плеер из папки его исходников:

```sh
cargo build --release --bins --all-features
```

Готовые программы находятся в target/release.
