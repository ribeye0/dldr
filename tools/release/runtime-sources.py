#!/usr/bin/env python3
"""Include pinned outer-runtime sources and its explicitly pinned library sources."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys

manifest = json.loads(Path(__file__).with_name('tools.json').read_text())
cache = Path(os.environ.get('XDG_CACHE_HOME') or Path.home() / '.cache') / 'tauri'
destination = Path(sys.argv[1]) / 'appimage-runtime'
destination.mkdir(parents=True, exist_ok=True)
for name, item in manifest.items():
    if item.get('sourceArchive'):
        source = cache / name
        assert hashlib.sha256(source.read_bytes()).hexdigest() == item['sha256'], name
        shutil.copyfile(source, destination / name)
(destination / 'tools.json').write_text(json.dumps(manifest, indent=2) + '\n')
commit = manifest['runtime-x86_64']['sourceCommit']
(destination / 'README.txt').write_text(
    f'AppImage outer runtime source commit: {commit}\n'
    'Unpack type2-runtime-source.tar.gz and consult BUILD.md and scripts/docker/.\n'
    f'The generated src/runtime/version file should contain {commit}.\n'
    'FUSE and squashfuse source archives match scripts/common/install-dependencies.sh,\n'
    'which includes its libfuse patch. All those scripts and patches are in the source archive.\n'
    'The upstream Alpine build installs the remaining standard toolchain/libraries listed\n'
    'in scripts/docker/Dockerfile. They can be replaced to rebuild/relink the runtime.\n'
    'Their notices and library license texts accompany the AppImage notice archive.\n'
)
