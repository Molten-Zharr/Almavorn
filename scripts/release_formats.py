"""Package a prepared release as AppImage or a standalone Flatpak bundle."""
import json
import shutil
import subprocess
from xml.sax.saxutils import escape


def run(*args, cwd=None):
    subprocess.run([str(arg) for arg in args], cwd=cwd, check=True)


def build_appimage(package, binary, stage, linuxdeploy, appimagetool):
    appdir = stage / "Almavorn.AppDir"
    shutil.copytree(package, appdir / "usr")
    run(linuxdeploy, "--appimage-extract-and-run", "--appdir", appdir,
        "--executable", appdir / "usr/bin" / binary,
        "--desktop-file", appdir / "usr/share/applications" / f"{binary}.desktop",
        "--icon-file", appdir / "usr/share/icons/hicolor/scalable/apps" / f"{binary}.svg")
    artifact = stage / f"{package.name}.AppImage"
    run(appimagetool, "--appimage-extract-and-run", "--no-appstream", appdir, artifact)
    return artifact


def build_flatpak(package, binary, stage):
    app_id = "io.github.MoltenZharr.Almavorn" + (".TUI" if binary.endswith("-tui") else "")
    build = stage / "flatpak-build"
    repo = stage / "flatpak-repo"
    run("flatpak", "build-init", "--arch=x86_64", build, app_id,
        "org.freedesktop.Sdk", "org.freedesktop.Platform", "25.08")
    files = build / "files"
    # Keep exact sources and notices available in every distribution format.
    for path in package.iterdir():
        target = files / path.name
        if path.is_dir():
            shutil.copytree(path, target, dirs_exist_ok=True)
        else:
            shutil.copy2(path, target)
    desktop = files / "share/applications" / f"{binary}.desktop"
    desktop.write_text(desktop.read_text().replace(f"Icon={binary}", f"Icon={app_id}"))
    desktop.rename(desktop.with_name(f"{app_id}.desktop"))
    icon = files / "share/icons/hicolor/scalable/apps" / f"{binary}.svg"
    icon.rename(icon.with_name(f"{app_id}.svg"))
    info = json.loads((package / "BUILD_INFO.json").read_text())
    metainfo = files / "share/metainfo" / f"{app_id}.metainfo.xml"
    metainfo.parent.mkdir(parents=True)
    metainfo.write_text(f'''<?xml version="1.0" encoding="UTF-8"?>
<component type="desktop-application">
  <id>{app_id}</id>
  <metadata_license>CC0-1.0</metadata_license>
  <project_license>MIT</project_license>
  <name>Almavorn{' TUI' if binary.endswith('-tui') else ''}</name>
  <summary>Organize your music and create flexible playlists</summary>
  <description><p>A local music player with protected playlists, free mixes,
  audio tag editing, themes and an equalizer.</p></description>
  <launchable type="desktop-id">{app_id}.desktop</launchable>
  <url type="homepage">https://github.com/Molten-Zharr/Almavorn</url>
  <releases><release version="{escape(info['version'])}"/></releases>
</component>
''')
    # Audio files can be anywhere, including removable drives. Tag editing needs writes.
    permissions = ["--socket=pulseaudio", "--filesystem=host",
                   "--own-name=org.mpris.MediaPlayer2.almavorn.*"]
    if binary == "almavorn":
        permissions += ["--socket=wayland", "--socket=fallback-x11", "--share=ipc", "--device=dri"]
    run("flatpak", "build-finish", f"--command={binary}", *permissions, build)
    run("flatpak", "build-export", "--arch=x86_64", repo, build, "stable")
    artifact = stage / f"{package.name}.flatpak"
    run("flatpak", "build-bundle", "--arch=x86_64",
        "--runtime-repo=https://dl.flathub.org/repo/flathub.flatpakrepo",
        repo, artifact, app_id, "stable")
    return artifact
