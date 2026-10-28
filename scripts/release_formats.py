"""Package a prepared release as AppImage or a standalone Flatpak bundle."""
import json
import shutil
import subprocess
from xml.sax.saxutils import escape


def run(*args, cwd=None):
    subprocess.run([str(arg) for arg in args], cwd=cwd, check=True)


def build_appimage(package, binary, stage, linuxdeploy, appimagetool):
    if shutil.which("strip") is None:
        raise RuntimeError("AppImage requires the system strip tool from binutils")
    # The bundled strip predates RELR sections used by current system libraries.
    # Use the host toolchain while preserving the downloaded packaging tool.
    tool_directory = stage / "linuxdeploy-tool"
    tool_directory.mkdir()
    subprocess.run([str(linuxdeploy), "--appimage-extract"], cwd=tool_directory,
                   stdout=subprocess.DEVNULL, check=True)
    extracted = tool_directory / "squashfs-root"
    bundled_strip = extracted / "usr/bin/strip"
    if bundled_strip.exists() or bundled_strip.is_symlink():
        bundled_strip.rename(bundled_strip.with_name("strip.bundled"))
    linuxdeploy = extracted / "AppRun"
    appdir = stage / "Almavorn.AppDir"
    shutil.copytree(package, appdir / "usr")
    libraries = []
    if binary == "almavorn":
        # Winit loads these at runtime, so an ELF dependency scan cannot find them.
        cache = subprocess.check_output(["ldconfig", "-p"], text=True).splitlines()
        for name in ("libxkbcommon.so.0", "libxkbcommon-x11.so.0", "libwayland-client.so.0",
                     "libwayland-cursor.so.0", "libwayland-egl.so.1", "libX11.so.6",
                     "libXcursor.so.1", "libXrandr.so.2", "libXi.so.6", "libXinerama.so.1"):
            path = next((line.split("=>", 1)[1].strip() for line in cache
                         if line.strip().startswith(name + " ") and "x86-64" in line), None)
            if path is None:
                raise RuntimeError(f"Missing AppImage library: {name}")
            libraries += ["--library", path]
    run(linuxdeploy, "--appdir", appdir,
        "--executable", appdir / "usr/bin" / binary,
        "--desktop-file", appdir / "usr/share/applications" / f"{binary}.desktop",
        "--icon-file", appdir / "usr/share/icons/hicolor/scalable/apps" / f"{binary}.svg", *libraries)
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
                   "--share=network", "--talk-name=org.freedesktop.secrets",
                   "--talk-name=org.freedesktop.FileManager1",
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
