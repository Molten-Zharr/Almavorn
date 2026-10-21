#!/usr/bin/env python3
"""Install an unpacked Linux release for the current user without sudo."""
import hashlib
import fcntl
import json
import os
from pathlib import Path
import re
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


def previous_installation(prefix, links, desktop, icon):
    paths = (*links, desktop, icon)
    if not any(os.path.lexists(path) for path in paths):
        return None
    old = None
    for link in links:
        if not link.is_symlink():
            raise ValueError(f"Путь занят посторонним файлом: {link}")
        target = link.resolve(strict=True)
        version_dir = target.parent.parent
        if (target.parent.name != "bin" or target.name != link.name
                or version_dir.parent != (prefix / "opt/almavorn").resolve()):
            raise ValueError(f"Команда указывает за пределы установки Almavorn: {link}")
        if old is not None and old != version_dir:
            raise ValueError("Команды Almavorn указывают на разные версии")
        old = version_dir
    info = json.loads((old / "BUILD_INFO.json").read_text(encoding="utf-8"))
    if info["version"] != old.name:
        raise ValueError("Версия прежней установки не совпадает с BUILD_INFO.json")
    for link in links:
        if digest(link) != info["binaries"][link.name]:
            raise ValueError(f"Прежняя команда изменена: {link}. Обновление остановлено.")
    if desktop.is_symlink() or icon.is_symlink():
        raise ValueError("Пункт меню или значок является посторонней ссылкой")
    expected = old / "bin/almavorn"
    text = desktop.read_text(encoding="utf-8")
    if (f"Exec={desktop_argument(expected)} %F\n" not in text
            or f"TryExec={expected}\n" not in text
            or digest(icon) != digest(old / "share/icons/hicolor/scalable/apps/almavorn.svg")):
        raise ValueError("Пункт меню или значок изменен вручную; обновление остановлено")
    return old


def install(source, info, prefix):
    version = info["version"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+(?:[-+][0-9A-Za-z.-]+)?", version):
        raise ValueError("Некорректная версия в BUILD_INFO.json")
    destination = prefix / "opt/almavorn" / version
    if set(info["binaries"]) != {"almavorn", "almavorn-tui"}:
        raise ValueError("В выпуске должны быть графическая и консольная версии")
    links = {prefix / "bin" / name: destination / "bin" / name for name in info["binaries"]}
    desktop = prefix / "share/applications/almavorn.desktop"
    icon = prefix / "share/icons/hicolor/scalable/apps/almavorn.svg"
    for name, checksum in info["binaries"].items():
        if digest(source / "bin" / name) != checksum:
            raise ValueError(f"Поврежден файл {name}: контрольная сумма не совпадает")
    old = previous_installation(prefix, links, desktop, icon)
    if os.path.lexists(destination):
        if old == destination.resolve() and all(digest(links_path) == info["binaries"][links_path.name] for links_path in links):
            print("Эта сборка Almavorn уже установлена.")
            return
        raise FileExistsError(f"Версия уже установлена или путь занят: {destination}")
    backup = old / f"launchers-before-{version}" if old else None
    if backup is not None and os.path.lexists(backup):
        raise FileExistsError(f"Резервные файлы уже существуют: {backup}")
    text = (source / "share/applications/almavorn.desktop").read_text(encoding="utf-8")
    executable = destination / "bin/almavorn"
    text = text.replace("Exec=almavorn %F", f"Exec={desktop_argument(executable)} %F")
    text = text.replace("TryExec=almavorn", f"TryExec={executable}")
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, destination)
    moved = []
    created = []
    try:
        if backup is not None:
            backup.mkdir()
            for path in (*links, desktop, icon):
                saved = backup / path.name
                path.rename(saved)
                moved.append((path, saved))
        for link, target in links.items():
            link.parent.mkdir(parents=True, exist_ok=True)
            link.symlink_to(target)
            created.append(link)
        icon.parent.mkdir(parents=True, exist_ok=True)
        with icon.open("xb") as handle:
            created.append(icon)
            with (source / "share/icons/hicolor/scalable/apps/almavorn.svg").open("rb") as original:
                shutil.copyfileobj(original, handle)
        desktop.parent.mkdir(parents=True, exist_ok=True)
        with desktop.open("x", encoding="utf-8") as handle:
            created.append(desktop)
            handle.write(text)
    except OSError:
        # Preserve generated files as well; restore launchers without deleting data.
        if created:
            failed = destination / "incomplete-launchers"
            failed.mkdir()
            for path in created:
                path.rename(failed / path.name)
        for path, saved in reversed(moved):
            if os.path.lexists(path):
                raise FileExistsError(f"Не удалось вернуть {path}: путь занят; прежний файл сохранен в {saved}")
            saved.rename(path)
        raise
    print(f"Almavorn {version} установлен: {destination}")
    if old is not None:
        print(f"Прежняя версия сохранена: {old}")
        print(f"Прежние файлы запуска сохранены: {backup}")
    print("Запуск: меню приложений -> Almavorn")
    print(f"Консольная версия: {prefix / 'bin/almavorn-tui'}")


def main():
    source = Path(__file__).resolve().parent
    info = json.loads((source / "BUILD_INFO.json").read_text(encoding="utf-8"))
    prefix = Path.home() / ".local"
    root = prefix / "opt/almavorn"
    root.mkdir(parents=True, exist_ok=True)
    with (root / ".install.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        install(source, info, prefix)


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError) as error:
        sys.exit(f"Almavorn: {error}")
