#!/usr/bin/env python3
"""Install an unpacked Linux release for the current user without sudo."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def desktop_argument(path):
    # Desktop Entry Exec quoting is different from shell quoting.
    value = str(path).replace("%", "%%")
    for char in ("\\", '"', "`", "$"):
        value = value.replace(char, "\\" + char)
    return '"' + value.replace("\\", "\\\\") + '"'


def main():
    source = Path(__file__).resolve().parent
    info = json.loads((source / "BUILD_INFO.json").read_text())
    version = info["version"]
    if not version or any(c not in "0123456789.-abcdefghijklmnopqrstuvwxyz" for c in version):
        raise ValueError("Некорректная версия в BUILD_INFO.json")
    prefix = Path.home() / ".local"
    destination = prefix / "opt/almavorn" / version
    links = {prefix / "bin" / name: destination / "bin" / name for name in info["binaries"]}
    desktop = prefix / "share/applications/almavorn.desktop"
    icon = prefix / "share/icons/hicolor/scalable/apps/almavorn.svg"
    # Preflight every destination; never replace another installation silently.
    for path in (destination, desktop, icon, *links):
        if os.path.lexists(path):
            raise FileExistsError(f"Путь уже занят: {path}. Установка остановлена.")
    for name, checksum in info["binaries"].items():
        if name not in ("almavorn", "almavorn-tui"):
            raise ValueError("Неизвестный исполняемый файл в BUILD_INFO.json")
        if digest(source / "bin" / name) != checksum:
            raise ValueError(f"Поврежден файл {name}: контрольная сумма не совпадает")
    text = (source / "share/applications/almavorn.desktop").read_text()
    executable = destination / "bin/almavorn"
    text = text.replace("Exec=almavorn %F", f"Exec={desktop_argument(executable)} %F")
    text = text.replace("TryExec=almavorn", f"TryExec={executable}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, destination)
    for link, target in links.items():
        link.parent.mkdir(parents=True, exist_ok=True)
        link.symlink_to(target)
    icon.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source / "share/icons/hicolor/scalable/apps/almavorn.svg", icon)
    desktop.parent.mkdir(parents=True, exist_ok=True)
    with desktop.open("x") as handle:
        handle.write(text)
    print(f"Almavorn {version} установлен: {destination}")
    print("Запуск: меню приложений -> Almavorn")
    print(f"Консольная версия: {prefix / 'bin/almavorn-tui'}")


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        sys.exit(f"Almavorn: {error}")
