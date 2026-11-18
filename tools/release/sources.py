#!/usr/bin/env python3
"""Download exact corresponding distro sources and vendor the Rust dependency graph."""
import argparse
import json
from pathlib import Path
import subprocess
import os
from concurrent.futures import ThreadPoolExecutor
import shutil
import re
import hashlib


def collect(inventory, target):
    packages = json.loads(inventory.read_text())
    target.mkdir(parents=True, exist_ok=True)
    cache = Path(os.environ.get("DLDR_SOURCE_CACHE", str(target.parent / "source-cache"))).resolve()
    cache.mkdir(parents=True, exist_ok=True)
    sources = sorted({(p["source"], p["sourceVersion"]) for p in packages})
    def download(item):
        name, version = item
        result = subprocess.run(["apt-get", "-o", "APT::Sandbox::User=root", "source", "--download-only", "--only-source", f"{name}={version}"], cwd=cache, text=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        if result.returncode:
            raise RuntimeError(f"Source download failed for {name}={version}:\n{result.stdout}")
        print(f"Source ready: {name} {version}", flush=True)
    # Source archives have package/version-qualified names; apt revalidates
    # cached files against repository checksums and never modifies dpkg state.
    with ThreadPoolExecutor(max_workers=6) as pool:
        list(pool.map(download, sources))
    system = target / "system"
    system.mkdir(exist_ok=True)
    required = set()
    for name, version in sources:
        descriptor = cache / f"{name}_{version.split(':')[-1]}.dsc"
        required.add(descriptor)
        checksums = re.search(r"^Checksums-Sha256:\n((?:[ \t].+\n)+)", descriptor.read_text(), re.M)
        if not checksums:
            raise RuntimeError(f"No SHA256 source manifest: {descriptor}")
        for line in checksums[1].splitlines():
            digest, size, filename = line.split()
            if Path(filename).name != filename:
                raise RuntimeError(f"Invalid source filename: {filename}")
            path = cache / filename
            with path.open('rb') as stream:
                actual = hashlib.file_digest(stream, 'sha256').hexdigest()
            if path.stat().st_size != int(size) or actual != digest:
                raise RuntimeError(f"Corresponding source checksum mismatch: {path}")
            required.add(path)
    for path in sorted(required):
        shutil.copyfile(path, system / path.name)
    with (target / "cargo-config.toml").open("w") as config:
        subprocess.run(["cargo", "vendor", "--locked", "--offline", str(target / "rust")], stdout=config, check=True)
    (target / "README.txt").write_text(
        "Corresponding sources for the packages listed in system-packages.json.\n"
        "Each distro .dsc file identifies the original source and packaging/build files.\n"
        "Use dpkg-source -x PACKAGE.dsc to unpack; consult debian/rules and debian/control.\n"
        "Rust dependencies are under rust/; cargo-config.toml is the vendor configuration.\n"
        "Before reuse, update its directory entry to your extracted rust/ directory.\n"
        "DLDR source and exact build scripts are in the accompanying dldr source archive.\n"
        "AppImage ELF RPATH changes and the Wayland-library exclusion are in tools/release/build-linux.sh.\n"
        "All components retain their own licenses.\n"
    )


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("inventory", type=Path)
    parser.add_argument("target", type=Path)
    args = parser.parse_args()
    collect(args.inventory.resolve(), args.target.resolve())
