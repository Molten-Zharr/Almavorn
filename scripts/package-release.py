#!/usr/bin/env python3
"""Build and package both Linux interfaces with licenses and matching sources."""
import argparse
import hashlib
import json
from pathlib import Path
import platform
import shutil
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[1]


def output(*args):
    return subprocess.check_output(args, cwd=ROOT, text=True).strip()


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--install", action="store_true", help="Build and install the current source revision for this user")
    options = parser.parse_args()
    if platform.system() != "Linux" or platform.machine() != "x86_64":
        raise SystemExit("Этот выпуск предназначен для Linux x86_64.")
    subprocess.run(["git", "diff", "--quiet", "HEAD", "--"], cwd=ROOT, check=True)
    metadata = json.loads(output("cargo", "metadata", "--locked", "--all-features",
                                 "--format-version", "1", "--filter-platform", "x86_64-unknown-linux-gnu"))
    app = next(p for p in metadata["packages"] if p["id"] == metadata["resolve"]["root"])
    version = app["version"]
    target = Path(metadata["target_directory"])
    dist = target / "dist" / version
    if dist.exists() and not options.install:
        raise SystemExit(f"Каталог выпуска уже существует: {dist}")
    subprocess.run(["cargo", "build", "--locked", "--release", "--bins", "--all-features", "--jobs", "4"],
                   cwd=ROOT, check=True)
    commit = output("git", "rev-parse", "HEAD")
    if options.install:
        fingerprint = hashlib.sha256("".join(sha256(target / "release" / name)
                                              for name in ("almavorn", "almavorn-tui")).encode()).hexdigest()[:8]
        version = f"{version}+local.{commit[:8]}.{fingerprint}"
    name = f"almavorn-{version}-linux-x86_64"
    with tempfile.TemporaryDirectory(prefix="almavorn-release-", dir=target) as temporary:
        stage = Path(temporary)
        package = stage / name
        (package / "bin").mkdir(parents=True)
        hashes = {}
        for binary in ("almavorn", "almavorn-tui"):
            path = package / "bin" / binary
            shutil.copy2(target / "release" / binary, path)
            hashes[binary] = sha256(path)
        for path in ("LICENSE", "README.md", "README.ru.md"):
            shutil.copy2(ROOT / path, package / path)
        shutil.copy2(ROOT / "scripts/install-user.py", package / "install.py")
        shutil.copy2(ROOT / "docs/RELEASE.md", package / "RELEASE.md")
        (package / "docs").mkdir()
        for filename in ("RELEASE.md", "GLOBAL_SHORTCUTS.md"):
            shutil.copy2(ROOT / "docs" / filename, package / "docs" / filename)
        for relative in ("applications/almavorn.desktop", "icons/hicolor/scalable/apps/almavorn.svg"):
            destination = package / "share" / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(ROOT / "packaging" / destination.name, destination)
        source = package / "share/source"
        source.mkdir(parents=True)
        subprocess.run(["git", "archive", "--format=tar.gz", f"--prefix=almavorn-{version}/",
                        f"--output={source / f'almavorn-{version}-source.tar.gz'}", commit], cwd=ROOT, check=True)
        licenses = package / "share/licenses"
        shutil.copytree(ROOT / "assets/fonts", licenses / "fonts", ignore=shutil.ignore_patterns("*.ttf"))
        resolved = {node["id"] for node in metadata["resolve"]["nodes"]}
        packages = sorted((p for p in metadata["packages"] if p["id"] in resolved and p["source"]),
                          key=lambda p: (p["name"], p["version"]))
        notices = ["# Included dependencies", "",
                   "Almavorn is MIT licensed. Dependencies retain their own licenses.",
                   "All resolved dependency sources, including statically linked SoundTouch (LGPL-2.1)",
                   "and Symphonia (MPL-2.0), are in share/source/dependencies.tar.gz.",
                   "License and copyright files are preserved there; extracted notices are in share/licenses.",
                   "See RELEASE.md for rebuilding and replacing SoundTouch. No dependency source is modified.", ""]
        with tarfile.open(source / "dependencies.tar.gz", "w:gz") as archive:
            for dependency in packages:
                label = f"{dependency['name']}-{dependency['version']}"
                crate = Path(dependency["manifest_path"]).parent
                archive.add(crate, arcname=label)
                notices.append(f"- {label}: {dependency['license'] or 'see source'}; "
                               f"{dependency['repository'] or 'https://crates.io/crates/' + dependency['name']}")
                for path in crate.rglob("*"):
                    upper = path.name.upper()
                    is_notice = any(word in upper for word in ("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE"))
                    is_font_notice = path.suffix == ".txt" and "fonts" in path.parts
                    if path.is_file() and (is_notice or is_font_notice):
                        destination = licenses / label / path.relative_to(crate)
                        destination.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copy2(path, destination)
        # Preserve the compiler's runtime notices when distributed by the toolchain.
        sysroot = Path(output("rustc", "--print", "sysroot"))
        for path in (sysroot / "share/doc/rust/COPYRIGHT.html", Path("/usr/share/licenses/rust/COPYRIGHT-library.html.rustc")):
            if path.is_file():
                shutil.copy2(path, licenses / "Rust-COPYRIGHT.html")
                break
        (package / "THIRD_PARTY_NOTICES.md").write_text("\n".join(notices) + "\n")
        info = {"version": version, "commit": commit, "target": "x86_64-unknown-linux-gnu",
                "rustc": output("rustc", "--version"), "libc": list(platform.libc_ver()),
                "binaries": hashes}
        (package / "BUILD_INFO.json").write_text(json.dumps(info, indent=2) + "\n")
        if options.install:
            subprocess.run([sys.executable, str(package / "install.py")], check=True)
            return
        filename = f"{name}.tar.gz"
        archive_path = stage / filename
        with tarfile.open(archive_path, "w:gz") as archive:
            archive.add(package, arcname=name)
        checksums = stage / "SHA256SUMS"
        checksums.write_text(f"{sha256(archive_path)}  {filename}\n")
        dist.mkdir(parents=True)
        shutil.move(archive_path, dist / filename)
        shutil.move(checksums, dist / "SHA256SUMS")
    print(f"Выпуск: {dist / filename}")


if __name__ == "__main__":
    main()
