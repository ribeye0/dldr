#!/usr/bin/env python3
"""Build a Windows x64 test candidate: portable ZIP, CLI and optional NSIS installer."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request
import zipfile

from notices import collect_code_notices

ROOT = Path(__file__).resolve().parents[2]
TARGET = 'x86_64-pc-windows-msvc'
RUNTIME_URL = ('https://github.com/mpv-player/mpv/releases/download/git-release/'
               'libmpv-v0.42.0-dev-g543a8c2c5-43736525345-x86_64-w64-mingw32-lgpl.zip')
RUNTIME_SHA256 = '7559e1fa0415866961611e6f5cf120c3ce43c8867b78fa43f825d704662c8845'


def run(*args, capture=False):
    if capture:
        return subprocess.check_output(args, text=True).strip()
    subprocess.run(args, check=True)


def cross_unrar(cache):
    """Keep cross-only patches in a private copy; never edit Cargo's registry."""
    metadata = json.loads(run('cargo', 'metadata', '--locked', '--offline', '--format-version', '1', capture=True))
    package = next(p for p in metadata['packages'] if p['name'] == 'unrar-ng-sys')
    if package['version'] != '0.7.7':
        raise RuntimeError('Review the UnRAR cross-build workaround for the new dependency version.')
    destination = cache / 'unrar-ng-sys'
    if destination.exists():
        shutil.rmtree(destination)
    shutil.copytree(Path(package['manifest_path']).parent, destination)
    script = destination / 'build.rs'
    text = script.read_text()
    if text.count('#[cfg(windows)]') != 2 or 'if cfg!(windows)' not in text:
        raise RuntimeError('Unrecognized UnRAR build script.')
    text = text.replace('cfg!(windows)', 'std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows")')
    text = text.replace('cfg!(target_env = "gnu")', 'std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("gnu")')
    text = text.replace('#[cfg(windows)]', '')
    script.write_text(text)
    # The SDK is case sensitive on Linux. Resolve include spellings against the
    # actual xwin SDK, rather than relying on Windows filesystem behavior.
    sdk = Path(os.environ['XWIN_CACHE_DIR']) / 'xwin'
    headers = [sdk / 'sdk/include' / part for part in ['um', 'shared', 'ucrt', 'winrt']]
    headers.append(sdk / 'crt/include')
    headers.extend([sdk / 'sdk/lib/um/x86_64', sdk / 'crt/lib/x86_64'])
    lookup = {p.name.lower(): p.name for folder in headers for p in folder.glob('*') if p.is_file()}
    if not lookup:
        raise RuntimeError('Run cargo xwin cache xwin before building on Linux.')
    for path in (destination / 'vendor/unrar').iterdir():
        if path.suffix not in {'.hpp', '.cpp', '.h'}:
            continue
        text = path.read_text()
        text = re.sub(r'(#\s*include\s*[<"])([^>"\n]+)([>"])',
                      lambda m: m[1] + lookup.get(m[2].lower(), m[2]) + m[3], text)
        text = re.sub(r'(#pragma\s+comment\s*\(\s*lib,\s*")([^"]+)(")',
                      lambda m: m[1] + lookup.get(m[2].lower(), m[2]) + m[3], text)
        if path.name == 'os.hpp':
            # MSVC allows these intrinsics without per-function target flags;
            # Clang does not. Retain the portable routines on cross builds.
            text = text.replace('  typedef const wchar* MSGID;', '#undef USE_SSE\n  typedef const wchar* MSGID;')
        path.write_text(text)
    return destination


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reuse-build', action='store_true', help='Package existing, unchanged Windows binaries')
    parser.add_argument('--no-installer', action='store_true', help='Only produce the portable ZIP and executables')
    parser.add_argument('--output', type=Path, default=ROOT / 'target/dist/windows')
    parser.add_argument('--cache', type=Path, default=Path(os.environ.get('XDG_CACHE_HOME', str(Path.home() / '.cache'))) / 'dldr-windows')
    args = parser.parse_args()
    os.chdir(ROOT)
    version = run(sys.executable, 'tools/release/validate.py', capture=True)
    output, cache = args.output.resolve(), args.cache.resolve()
    output.mkdir(parents=True, exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / 'libmpv-lgpl.zip'
    if not archive.is_file():
        temporary = archive.with_suffix('.download')
        urllib.request.urlretrieve(RUNTIME_URL, temporary)
        temporary.replace(archive)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != RUNTIME_SHA256:
        raise RuntimeError('libmpv archive checksum mismatch')
    stage = output / 'DLDR'
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir()
    notices = stage / 'third-party'
    with zipfile.ZipFile(archive) as package:
        for source, target in [('libmpv-2.dll', stage / 'libmpv-2.dll'),
                               ('LICENSE.LGPL', notices / 'mpv/LICENSE.LGPL')]:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(package.read(source))
    rust, javascript = collect_code_notices(notices, TARGET)
    for name, data in [('rust-packages.json', rust), ('javascript-packages.json', javascript)]:
        (notices / name).write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    shutil.copyfile(ROOT / 'docs/THIRD-PARTY.md', notices / 'THIRD-PARTY.md')
    runtime = {'url': RUNTIME_URL, 'sha256': RUNTIME_SHA256, 'mpvRevision': '543a8c2c5',
               'variant': 'first-party Windows x64 LGPL', 'replaceable': 'libmpv-2.dll beside the executable'}
    (notices / 'windows-runtime.json').write_text(json.dumps(runtime, indent=2) + '\n')
    config = {'bundle': {'targets': ['nsis'], 'resources': {
        str(stage / 'libmpv-2.dll'): 'libmpv-2.dll', str(stage / 'dldr.exe'): 'dldr.exe', str(notices): 'third-party'},
        'windows': {'webviewInstallMode': {'type': 'downloadBootstrapper'},
                    'nsis': {'installMode': 'currentUser', 'displayLanguageSelector': True,
                             'languages': ['TradChinese', 'SimpChinese', 'Japanese', 'English']}}}}
    native = sys.platform == 'win32'
    pnpm = shutil.which('pnpm.cmd' if native else 'pnpm')
    if not pnpm:
        raise RuntimeError('pnpm is required')
    target_dir = Path(json.loads(run('cargo', 'metadata', '--no-deps', '--format-version', '1', capture=True))['target_directory'])
    binaries = target_dir / TARGET / 'release'
    cargo_args = []
    if not native and not args.reuse_build:
        patch = cross_unrar(cache)
        cargo_args = ['--config', 'paths=' + json.dumps([str(patch)])]
    with tempfile.TemporaryDirectory(prefix='dldr-windows-') as temporary:
        config_path = Path(temporary) / 'tauri.windows.json'
        config_path.write_text(json.dumps(config), encoding='utf-8')
        if not args.reuse_build:
            runner = ['cargo', 'build'] if native else ['cargo', 'xwin', 'build']
            run(*runner, '--locked', '--release', '--target', TARGET, '-p', 'dldr-cli', *cargo_args)
            shutil.copyfile(binaries / 'dldr.exe', stage / 'dldr.exe')
            command = [pnpm, 'tauri', 'build', '--target', TARGET, '--config', str(config_path)]
            command += ['--no-bundle'] if args.no_installer else ['--bundles', 'nsis']
            if not native:
                command += ['--runner', 'cargo-xwin']
            run(*command, *(['--'] + cargo_args if cargo_args else []))
        elif not args.no_installer:
            shutil.copyfile(binaries / 'dldr.exe', stage / 'dldr.exe')
            run(pnpm, 'tauri', 'bundle', '--target', TARGET, '--bundles', 'nsis', '--config', str(config_path))
    for source, target in [('dldr-desktop.exe', 'DLDR.Desktop.exe'), ('dldr.exe', 'dldr.exe')]:
        binary = binaries / source
        if not binary.is_file() or binary.read_bytes()[:2] != b'MZ':
            raise RuntimeError(f'Windows executable missing: {binary}')
        shutil.copyfile(binary, stage / target)
    shutil.copyfile(ROOT / 'LICENSE.md', stage / 'LICENSE.md')
    shutil.copyfile(ROOT / 'docs/windows.md', stage / 'README.md')
    info = {'version': version, 'target': TARGET, 'commit': run('git', 'rev-parse', 'HEAD', capture=True),
            'dirty': bool(run('git', 'status', '--porcelain', capture=True)), 'runtime': runtime,
            'nativeWindowsAcceptance': 'pending', 'distribution': 'local test candidate',
            'rust': run('rustc', '--version', capture=True)}
    (stage / 'build-info.json').write_text(json.dumps(info, indent=2) + '\n')
    zip_path = output / f'DLDR_{version}_windows_x64.zip'
    # Windows treats DLDR.exe and dldr.exe as the same file. Check the whole
    # payload, including notices, before emitting a portable archive.
    names = set()
    with zipfile.ZipFile(zip_path, 'w', compression=zipfile.ZIP_DEFLATED, compresslevel=6) as package:
        for path in sorted(stage.rglob('*')):
            if path.is_file():
                name = 'DLDR/' + path.relative_to(stage).as_posix()
                if name.casefold() in names:
                    raise RuntimeError(f'Windows filename collision: {name}')
                names.add(name.casefold())
                package.write(path, name)
    if not args.no_installer:
        installers = list((binaries / 'bundle/nsis').glob('*setup.exe'))
        if len(installers) != 1:
            raise RuntimeError(f'Expected one NSIS installer, found {installers}')
        shutil.copyfile(installers[0], output / f'DLDR_{version}_x64-setup.exe')
    # Current source files only: deleted historical files must not enter archives.
    files = run('git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', capture=True).split('\0')
    with tarfile.open(output / f'DLDR_{version}_windows-source.tar.gz', 'w:gz') as package:
        for name in sorted(set(files)):
            if name and Path(name).is_file():
                package.add(name, arcname='dldr/' + name, recursive=False)
    artifacts = [zip_path, output / f'DLDR_{version}_windows-source.tar.gz']
    if not args.no_installer:
        artifacts.append(output / f'DLDR_{version}_x64-setup.exe')
    (output / 'SHA256SUMS.txt').write_text(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in artifacts))
    print(f'Windows candidate: {zip_path}\nExecutable: {stage / "DLDR.Desktop.exe"}')


if __name__ == '__main__':
    main()
