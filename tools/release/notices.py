#!/usr/bin/env python3
"""Collect actual bundled system packages and dependency notices in an AppDir."""
import argparse
import hashlib
import json
from pathlib import Path
import shutil
import subprocess


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def collect_code_notices(doc, target=None):
    doc.mkdir(parents=True, exist_ok=True)
    fonts = doc / "fonts"
    fonts.mkdir(exist_ok=True)
    shutil.copyfile("apps/desktop/static/fonts/OFL.txt", fonts / "NotoSansCJK-OFL.txt")
    args = ["cargo", "metadata", "--locked", "--offline", "--format-version", "1"]
    if target:
        args += ["--filter-platform", target]
    cargo = json.loads(run(*args))
    rust = []
    for package in cargo["packages"]:
        rust.append({k: package.get(k) for k in ["name", "version", "license", "repository", "source"]})
        folder = Path(package["manifest_path"]).parent
        for path in folder.iterdir():
            if path.is_file() and path.name.lower().startswith(("license", "licence", "copying", "copyright", "notice")):
                dest = doc / "rust" / f'{package["name"]}-{package["version"]}'
                dest.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, dest / path.name)
        unrar_license = folder / "vendor/unrar/license.txt"
        if unrar_license.is_file():
            dest = doc / "rust" / f'{package["name"]}-{package["version"]}'
            dest.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(unrar_license, dest / "UnRAR-license.txt")
    javascript, seen = [], set()
    for manifest in Path("node_modules/.pnpm").glob("*/node_modules/**/package.json"):
        folder = manifest.parent.resolve()
        if folder in seen:
            continue
        seen.add(folder)
        try:
            data = json.loads(manifest.read_text())
        except (ValueError, OSError):
            continue
        if "name" not in data or "version" not in data:
            continue
        javascript.append({k: data.get(k) for k in ["name", "version", "license", "repository"]})
        dest = doc / "javascript" / (data["name"].replace("/", "_") + "-" + data["version"])
        for path in folder.iterdir():
            if path.is_file() and path.name.lower().startswith(("license", "licence", "copying", "copyright", "notice")):
                dest.mkdir(parents=True, exist_ok=True)
                shutil.copyfile(path, dest / path.name)
    return rust, javascript


def collect(appdir, output):
    doc = appdir / "usr/share/doc/dldr"
    doc.mkdir(parents=True, exist_ok=True)
    exact, basenames = {}, {}
    for listing in Path("/var/lib/dpkg/info").glob("*.list"):
        package = listing.name.removesuffix(".list")
        for line in listing.read_text().splitlines():
            exact.setdefault(line, set()).add(package)
            basenames.setdefault(Path(line).name, set()).add(package)
    owners, files, unknown = set(), [], []
    # Capture only application payload, not the notices added below.
    for path in sorted(appdir.rglob("*")):
        if doc in path.parents or not path.is_file() or path.is_symlink():
            continue
        relative = path.relative_to(appdir)
        with path.open("rb") as stream:
            elf = stream.read(4) == b"\x7fELF"
        packages = exact.get("/" + str(relative), set())
        if not packages and elf:
            packages = basenames.get(path.name, set())
        owners.update(packages)
        if elf and not packages and path.name not in ["dldr-desktop", "AppRun", "AppRun.wrapped"]:
            unknown.append(str(relative))
        files.append({"path": str(relative), "packages": sorted(packages)})
    if unknown:
        raise RuntimeError(f"Bundled ELF files without a source package: {unknown}")
    tools = json.loads(Path(__file__).with_name("tools.json").read_text())
    loader = appdir / ("AppRun.wrapped" if (appdir / "AppRun.wrapped").exists() else "AppRun")
    assert hashlib.sha256(loader.read_bytes()).hexdigest() == tools["AppRun-x86_64"]["sha256"], "Unknown AppRun loader"
    shutil.copyfile(Path(__file__).with_name("tools.json"), doc / "bundler-tools.json")
    shutil.copytree(Path(__file__).with_name("licenses"), doc / "bundler-licenses", dirs_exist_ok=True)
    inventory = []
    for package in sorted(owners):
        fields = run("dpkg-query", "-W", "-f=${binary:Package}\t${Version}\t${source:Package}\t${source:Version}", package).split("\t")
        inventory.append(dict(zip(["package", "version", "source", "sourceVersion"], fields)))
        copyright_file = Path("/usr/share/doc") / package.split(":")[0] / "copyright"
        if copyright_file.is_file():
            dest = doc / "system" / package
            dest.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(copyright_file, dest / "copyright")
    shutil.copytree("/usr/share/common-licenses", doc / "common-licenses", dirs_exist_ok=True)
    rust, javascript = collect_code_notices(doc)
    for name, value in [("system-packages.json", inventory), ("bundled-files.json", files), ("rust-packages.json", rust), ("javascript-packages.json", javascript)]:
        (doc / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n")
    shutil.copyfile("LICENSE.md", doc / "LICENSE.md")
    shutil.copyfile("docs/THIRD-PARTY.md", doc / "THIRD-PARTY.md")
    output.mkdir(parents=True, exist_ok=True)
    (output / "system-packages.json").write_text(json.dumps(inventory, indent=2) + "\n")
    print(f"Notices: {len(inventory)} system packages, {len(rust)} Rust crates, {len(javascript)} JavaScript packages")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("appdir", type=Path)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    collect(args.appdir, args.output)
