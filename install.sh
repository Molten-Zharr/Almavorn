#!/usr/bin/env bash
set -euo pipefail

fail() { printf 'Almavorn: %s\n' "$*" >&2; exit 1; }
repository='https://github.com/Molten-Zharr/Almavorn.git'
source_dir="$HOME/.local/src/almavorn"
variant=guitui
while (($#)); do
case "$1" in
    --help|-h)
        printf 'Установка Almavorn: bash install.sh [--variant guitui|tui] [--source-dir ПАПКА]\n'
        exit 0 ;;
    --source-dir)
        [[ $# -ge 2 && -n $2 ]] || fail 'Укажите папку исходников после --source-dir.'
        source_dir="$2"; shift 2 ;;
    --variant)
        [[ $# -ge 2 ]] || fail 'Выберите guitui или tui после --variant.'
        variant="$2"; shift 2 ;;
    *) fail 'Неизвестный аргумент. Используйте --help.' ;;
esac
done
[[ $variant == guitui || $variant == tui ]] || fail 'Доступны интерфейсы guitui и tui.'
[[ $(uname -s) == Linux && $(uname -m) == x86_64 ]] || fail 'Поддерживается Linux x86_64.'
[[ $EUID != 0 ]] || fail 'Запустите установку обычным пользователем, без sudo.'
[[ $source_dir == /* ]] || source_dir="$PWD/$source_dir"
if [[ -e $source_dir && ! -d $source_dir/.git ]]; then
    fail "Папка занята другими файлами: $source_dir. Выберите другую через --source-dir."
fi

if pgrep -u "$(id -u)" -x almavorn >/dev/null 2>&1 || pgrep -u "$(id -u)" -x almavorn-tui >/dev/null 2>&1; then
    fail 'Закройте плеер и повторите команду установки.'
fi

# Only install the build tools needed by the existing Rust and SoundTouch code.
. /etc/os-release
case " ${ID:-} ${ID_LIKE:-} " in
    *' debian '*|*' ubuntu '*)
        packages=(git curl ca-certificates python3 build-essential pkg-config libclang-dev libasound2-dev)
        if [[ $variant == guitui ]]; then
            packages+=(libxkbcommon-dev libwayland-dev libgl1-mesa-dev)
        fi
        missing=()
        for package in "${packages[@]}"; do
            [[ $(dpkg-query -W -f='${Status}' "$package" 2>/dev/null || true) == 'install ok installed' ]] || missing+=("$package")
        done
        if ((${#missing[@]})); then
            command -v sudo >/dev/null || fail 'Для установки зависимостей нужна команда sudo.'
            printf 'Устанавливаю необходимые инструменты.\n'
            sudo apt-get update
            sudo apt-get install -y --no-install-recommends "${missing[@]}"
        fi ;;
    *' arch '*|*' artix '*)
        packages=(git curl ca-certificates python base-devel clang pkgconf alsa-lib)
        if [[ $variant == guitui ]]; then
            packages+=(libxkbcommon wayland libglvnd)
        fi
        missing=()
        for package in "${packages[@]}"; do
            pacman -Q "$package" >/dev/null 2>&1 || missing+=("$package")
        done
        if ((${#missing[@]})); then
            command -v sudo >/dev/null || fail 'Для установки зависимостей нужна команда sudo.'
            printf 'Устанавливаю необходимые инструменты.\n'
            sudo pacman -S --needed --noconfirm "${missing[@]}"
        fi ;;
    *) fail 'Автоматическая установка поддерживает Ubuntu, Debian, Arch Linux и Artix.' ;;
esac

printf 'Подготавливаю исходники Almavorn.\n'
if [[ -d $source_dir/.git ]]; then
    [[ $(git -C "$source_dir" remote get-url origin) == "$repository" ]] || fail "В $source_dir находится другой репозиторий."
    [[ $(git -C "$source_dir" branch --show-current) == main ]] || fail 'В папке исходников выбрана другая ветка. Используйте отдельную папку через --source-dir.'
    [[ -z $(git -C "$source_dir" status --porcelain) ]] || fail 'В папке исходников есть ваши изменения. Сохраните их или выберите другую папку через --source-dir.'
    git -C "$source_dir" pull --ff-only
else
    mkdir -p "$(dirname "$source_dir")"
    git clone --branch main --single-branch "$repository" "$source_dir"
fi
cd "$source_dir"

# Reuse an adequate system toolchain; do not replace it or change shell files.
rust_ready=false
if command -v cargo >/dev/null && command -v rustc >/dev/null; then
    rust_version=$(rustc --version)
    if python3 -c 'import re,sys; m=re.match(r"rustc (\d+)\.(\d+)",sys.argv[1]); sys.exit(not (m and tuple(map(int,m.groups())) >= (1,98)))' "$rust_version"; then
        rust_ready=true
    fi
fi
if [[ $rust_ready == true ]]; then
    printf 'Собираю и устанавливаю плеер.\n'
    python3 scripts/package-release.py --install --variant "$variant"
else
    rustup_command=$(command -v rustup || true)
    if [[ -z $rustup_command ]]; then
        rustup_command="${CARGO_HOME:-$HOME/.cargo}/bin/rustup"
    fi
    if [[ ! -x $rustup_command ]]; then
        download_dir=$(mktemp -d /tmp/almavorn-rust.XXXXXX)
        curl --proto '=https' --tlsv1.2 -fsSL https://sh.rustup.rs -o "$download_dir/install-rust.sh"
        sh "$download_dir/install-rust.sh" -y --profile minimal --no-modify-path --default-toolchain stable
    fi
    "$rustup_command" toolchain install stable --profile minimal
    printf 'Собираю и устанавливаю плеер.\n'
    "$rustup_command" run stable python3 scripts/package-release.py --install --variant "$variant"
fi
if command -v update-desktop-database >/dev/null; then
    update-desktop-database "$HOME/.local/share/applications" || true
fi
if [[ $variant == tui ]]; then
    printf 'Готово. Запуск: %s/.local/bin/almavorn-tui\n' "$HOME"
else
    printf 'Готово. Откройте Almavorn в меню приложений.\n'
fi
