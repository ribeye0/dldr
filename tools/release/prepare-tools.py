#!/usr/bin/env python3
"""Seed Tauri's bundler cache with verified immutable release artifacts."""
import hashlib
import json
import os
from pathlib import Path
import urllib.request

manifest = json.loads(Path(__file__).with_name("tools.json").read_text())
cache = Path(os.environ.get("XDG_CACHE_HOME") or Path.home() / ".cache") / "tauri"
cache.mkdir(parents=True, exist_ok=True)
for name, item in manifest.items():
    # linuxdeploy is modified by Tauri after use, so redownload instead of
    # trusting a previously patched executable in a persistent runner cache.
    with urllib.request.urlopen(item["url"], timeout=120) as response:
        data = response.read()
    if hashlib.sha256(data).hexdigest() != item["sha256"]:
        raise RuntimeError(f"Bundler checksum mismatch: {name}")
    temporary = cache / (name + ".new")
    temporary.write_bytes(data)
    temporary.chmod(0o755)
    temporary.replace(cache / name)
    print(f"Verified bundler: {name}")
