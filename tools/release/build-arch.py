#!/usr/bin/env python3
"""Build a native Arch Linux package containing the desktop app and CLI."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

from notices import collect_code_notices
from validate import validate

ROOT = Path(__file__).resolve().parents[2]


def run(*args, capture=False, **kwargs):
    if capture:
        return subprocess.check_output(args, text=True, **kwargs).strip()
    subprocess.run(args, check=True, **kwargs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT / 'target/dist/arch')
    parser.add_argument('--reuse-build', action='store_true', help='Package unchanged native binaries')
    parser.add_argument('--pkgrel', type=int, default=1, help='Arch package revision (default: 1)')
    args = parser.parse_args()
    if args.pkgrel < 1:
        parser.error('--pkgrel must be a positive integer')
    os.chdir(ROOT)
    if 'ID=arch\n' not in Path('/etc/os-release').read_text() or os.uname().machine != 'x86_64':
        parser.error('Run this build on Arch Linux x86_64.')
    version = validate()
    target = Path(json.loads(run('cargo', 'metadata', '--no-deps', '--format-version', '1', capture=True))['target_directory'])
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    if not args.reuse_build:
        run('pnpm', 'tauri', 'build', '--no-bundle')
        run('cargo', 'build', '--locked', '--release', '-p', 'dldr-cli')
    with tempfile.TemporaryDirectory(prefix='dldr-arch-', dir=target) as temporary:
        work = Path(temporary)
        stage = work / 'payload'

        def copy(source, destination, mode=0o644):
            dest = stage / destination
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(source, dest)
            dest.chmod(mode)

        for binary in ['dldr-desktop', 'dldr']:
            copy(target / 'release' / binary, f'usr/bin/{binary}', 0o755)
        launcher = stage / 'usr/bin/dldr-gui'
        launcher.write_text('#!/bin/sh\nexec /usr/bin/dldr-desktop "$@"\n')
        launcher.chmod(0o755)
        entry = stage / 'usr/share/applications/io.github.ribeye0.dldr.desktop'
        entry.parent.mkdir(parents=True)
        entry.write_text('[Desktop Entry]\nType=Application\nName=DLDR\n'
                         'GenericName=DLsite Downloader\nExec=dldr-gui\n'
                         'Icon=io.github.ribeye0.dldr\nTerminal=false\n'
                         'Categories=AudioVideo;Audio;\nStartupWMClass=io.github.ribeye0.dldr\n')
        for size, source in [(32, '32x32.png'), (128, '128x128.png'), (256, '128x128@2x.png')]:
            copy(ROOT / 'apps/desktop/src-tauri/icons' / source,
                 f'usr/share/icons/hicolor/{size}x{size}/apps/io.github.ribeye0.dldr.png')
        copy(ROOT / 'LICENSE.md', 'usr/share/licenses/dldr/LICENSE.md')
        copy(ROOT / 'apps/desktop/static/fonts/OFL.txt', 'usr/share/licenses/dldr/NotoSansCJK-OFL.txt')
        doc = stage / 'usr/share/doc/dldr'
        rust, javascript = collect_code_notices(doc / 'third-party', 'x86_64-unknown-linux-gnu')
        for name, value in [('rust-packages.json', rust), ('javascript-packages.json', javascript)]:
            (doc / 'third-party' / name).write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n')
        copy(ROOT / 'docs/THIRD-PARTY.md', 'usr/share/doc/dldr/THIRD-PARTY.md')
        copy(ROOT / 'docs/linux-install.md', 'usr/share/doc/dldr/README.md')
        info = {'version': version, 'commit': run('git', 'rev-parse', 'HEAD', capture=True),
                'dirty': bool(run('git', 'status', '--porcelain', capture=True)),
                'rust': run('rustc', '--version', capture=True),
                'packages': run('pacman', '-Q', capture=True)}
        (doc / 'build-info.json').write_text(json.dumps(info, indent=2) + '\n')
        run('desktop-file-validate', str(entry))
        payload = work / 'payload.tar'
        run('tar', '-cf', str(payload), '-C', str(work), 'payload')
        digest = hashlib.file_digest(payload.open('rb'), 'sha256').hexdigest()
        recipe = (ROOT / 'tools/release/arch/PKGBUILD').read_text().replace('@VERSION@', version).replace('@PKGREL@', str(args.pkgrel)).replace('@SHA256@', digest)
        (work / 'PKGBUILD').write_text(recipe)
        env = os.environ | {'PKGDEST': str(output), 'PACKAGER': 'ribeye0',
                            'SOURCE_DATE_EPOCH': run('git', 'log', '-1', '--format=%ct', capture=True)}
        run('makepkg', '--clean', '--force', cwd=work, env=env)
    package = output / f'dldr-{version}-{args.pkgrel}-x86_64.pkg.tar.zst'
    run('pacman', '-Qip', str(package))
    (output / 'SHA256SUMS.txt').write_text(hashlib.file_digest(package.open('rb'), 'sha256').hexdigest() + '  ' + package.name + '\n')
    print(f'Arch package: {package}')


if __name__ == '__main__':
    main()
