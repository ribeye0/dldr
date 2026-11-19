#!/usr/bin/env python3
"""Reject mismatched GUI/CLI/tag versions before building release artifacts."""
import argparse
import json
from pathlib import Path
import re
import tomllib

ROOT = Path(__file__).resolve().parents[2]


def validate(tag=None):
    package = json.loads((ROOT / "package.json").read_text(encoding="utf-8"))
    config = json.loads((ROOT / "apps/desktop/src-tauri/tauri.conf.json").read_text(encoding="utf-8"))
    desktop = tomllib.loads((ROOT / "apps/desktop/src-tauri/Cargo.toml").read_text(encoding="utf-8"))
    workspace = tomllib.loads((ROOT / "Cargo.toml").read_text(encoding="utf-8"))["workspace"]["package"]
    if config["identifier"] != "io.github.ribeye0.dldr" or config["productName"] != "DLDR":
        raise ValueError("Desktop identity must use DLDR / io.github.ribeye0.dldr")
    if desktop["package"]["name"] != "dldr-desktop" or desktop["lib"]["name"] != "dldr_desktop_lib":
        raise ValueError("Native package and library names must use DLDR")
    versions = {"package.json": package["version"], "tauri.conf.json": config["version"], "desktop/package.json": json.loads((ROOT/"apps/desktop/package.json").read_text(encoding="utf-8"))["version"]}
    for file in ["apps/desktop/src-tauri/Cargo.toml", "apps/cli/Cargo.toml"]:
        value = tomllib.loads((ROOT / file).read_text(encoding="utf-8"))["package"]["version"]
        versions[file] = workspace["version"] if isinstance(value, dict) and value.get("workspace") else value
    if len(set(versions.values())) != 1:
        raise ValueError(f"Release versions differ: {versions}")
    version = config["version"]
    if not re.fullmatch(r"\d+\.\d+\.\d+(?:-rc\.\d+)?", version):
        raise ValueError(f"Unsupported release version: {version}")
    if tag is not None and tag != "v" + version:
        raise ValueError(f"Tag {tag!r} does not match v{version}")
    updater = config.get("plugins", {}).get("updater", {})
    if config["bundle"]["createUpdaterArtifacts"] is not True or not updater.get("pubkey") \
            or updater.get("endpoints") != ["https://github.com/ribeye0/dldr/releases/latest/download/latest.json"]:
        raise ValueError("Signed updater artifacts and the DLDR release endpoint must be configured")
    repository = "https://github.com/ribeye0/dldr"
    if package.get("homepage") != repository or f'GITHUB_URL = "{repository}"' not in (ROOT / "apps/desktop/src/lib/model/constants.ts").read_text(encoding="utf-8"):
        raise ValueError("Project/release links must point to ribeye0/dldr")
    locked = tomllib.loads((ROOT / "Cargo.lock").read_text(encoding="utf-8"))
    for name in ["dldr-desktop", "dldr-cli"]:
        if next(p["version"] for p in locked["package"] if p["name"] == name) != version:
            raise ValueError(f"Cargo.lock version differs for {name}")
    return version


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tag", nargs="?")
    args = parser.parse_args()
    try:
        print(validate(args.tag))
    except (ValueError, KeyError, StopIteration) as error:
        parser.exit(1, f"Release validation failed: {error}\n")
