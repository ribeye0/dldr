#!/usr/bin/env python3
"""Build Windows x64 releases with pinned runtimes and corresponding sources."""
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
from updater import check_key, sign, signing_environment

from concurrent.futures import ThreadPoolExecutor

from notices import collect_code_notices

ROOT = Path(__file__).resolve().parents[2]
TARGET = 'x86_64-pc-windows-msvc'


def run(*args, capture=False):
    if capture:
        return subprocess.check_output(args, text=True, encoding="utf-8").strip()
    subprocess.run(args, check=True)


def bundle_runtime(stage, cache, source_dir):
    """Ship pinned MSYS2 DLLs, notices and their exact source packages."""
    manifest = Path(__file__).with_name('windows-runtime.json')
    packages = json.loads(manifest.read_text(encoding="utf-8"))
    downloads = {}
    for package in packages:
        downloads[package['url']] = package['sha256']
        downloads[package['source']] = package['sourceSha256']

    def download(item):
        url, expected = item
        destination = cache / url.rsplit('/', 1)[1]
        if not destination.exists():
            temporary = destination.with_suffix('.download')
            with urllib.request.urlopen(url, timeout=120) as response, temporary.open('wb') as stream:
                shutil.copyfileobj(response, stream)
            temporary.replace(destination)
        with destination.open('rb') as stream:
            actual = hashlib.file_digest(stream, 'sha256').hexdigest()
        if actual != expected:
            raise RuntimeError(f'Runtime checksum mismatch: {destination.name}')

    with ThreadPoolExecutor(max_workers=6) as pool:
        list(pool.map(download, downloads.items()))
    for package in packages:
        archive = cache / package['url'].rsplit('/', 1)[1]
        # Both GNU tar (with zstd) and Windows 11's bsdtar support .tar.zst.
        members = run('tar', '-tf', str(archive), capture=True).splitlines()
        source = cache / package['source'].rsplit('/', 1)[1]
        shutil.copyfile(source, source_dir / source.name)
        for member in package['files']:
            if member not in members:
                raise RuntimeError(f'Missing runtime DLL: {member}')
            data = subprocess.check_output(['tar', '-xOf', str(archive), member])
            if data[:2] != b'MZ':
                raise RuntimeError(f'Invalid Windows DLL: {member}')
            (stage / Path(member).name).write_bytes(data)
        for member in members:
            if member.startswith('ucrt64/share/licenses/') and not member.endswith('/'):
                target = stage / 'third-party' / 'runtime' / member.removeprefix('ucrt64/share/licenses/')
                target.parent.mkdir(parents=True, exist_ok=True)
                target.write_bytes(subprocess.check_output(['tar', '-xOf', str(archive), member]))
    shutil.copyfile(manifest, stage / 'third-party' / 'windows-runtime.json')
    common = stage / 'third-party' / 'common-licenses'
    common.mkdir()
    for name in ['GPL-2', 'GPL-3', 'LGPL-2.1', 'LGPL-3']:
        shutil.copyfile(Path(__file__).with_name('licenses') / f'{name}.txt', common / f'{name}.txt')


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
    text = script.read_text(encoding="utf-8")
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
        text = path.read_text(encoding="utf-8")
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
    if not args.no_installer:
        os.environ.update(signing_environment())
    check_key()
    output, cache = args.output.resolve(), args.cache.resolve()
    output.mkdir(parents=True, exist_ok=True)
    cache.mkdir(parents=True, exist_ok=True)
    stage = output / 'DLDR'
    if stage.exists():
        shutil.rmtree(stage)
    stage.mkdir()
    notices = stage / 'third-party'
    notices.mkdir()
    source_dir = output / 'runtime-sources'
    if source_dir.exists():
        shutil.rmtree(source_dir)
    source_dir.mkdir()
    bundle_runtime(stage, cache, source_dir)
    rust, javascript = collect_code_notices(notices, TARGET)
    for name, data in [('rust-packages.json', rust), ('javascript-packages.json', javascript)]:
        (notices / name).write_text(json.dumps(data, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
    shutil.copyfile(ROOT / 'docs/THIRD-PARTY.md', notices / 'THIRD-PARTY.md')
    runtime = {'distribution': 'MSYS2 UCRT64', 'manifest': 'third-party/windows-runtime.json',
               'replaceable': 'DLLs beside the executable'}
    shutil.copyfile(ROOT / 'LICENSE.md', stage / 'LICENSE.md')
    shutil.copyfile(ROOT / 'docs/windows.md', stage / 'README.md')
    config = {'bundle': {'targets': ['nsis'], 'resources': {
        **{str(path): path.name for path in stage.glob('*.dll')},
        str(stage / 'LICENSE.md'): 'LICENSE.md', str(stage / 'README.md'): 'README.md',
        str(stage / 'dldr.exe'): 'dldr.exe', str(notices): 'third-party'},
        'windows': {'webviewInstallMode': {'type': 'downloadBootstrapper'},
                    'nsis': {'installMode': 'currentUser', 'displayLanguageSelector': True,
                             'installerIcon': str(ROOT / 'apps/desktop/src-tauri/icons/icon.ico'),
                             'uninstallerIcon': str(ROOT / 'apps/desktop/src-tauri/icons/icon.ico'),
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
        installers = list((binaries / 'bundle/nsis').glob(f'*_{version}_x64-setup.exe'))
        if len(installers) != 1:
            raise RuntimeError(f'Expected one NSIS installer, found {installers}')
        shutil.copyfile(installers[0], output / f'DLDR_{version}_x64-setup.exe')
    with zipfile.ZipFile(zip_path) as package:
        if damaged := package.testzip():
            raise RuntimeError(f'Corrupt portable ZIP member: {damaged}')
    # Current source files only: deleted historical files must not enter archives.
    files = run('git', 'ls-files', '--cached', '--others', '--exclude-standard', '-z', capture=True).split('\0')
    with tarfile.open(output / f'DLDR_{version}_windows-source.tar.gz', 'w:gz') as package:
        for name in sorted(set(files)):
            if name and Path(name).is_file():
                package.add(name, arcname='dldr/' + name, recursive=False)
    # Vendor the complete Rust graph, including native C/C++ crate sources.
    with (source_dir / 'cargo-config.toml').open('w', encoding='utf-8') as config_file:
        subprocess.run(['cargo', 'vendor', '--locked', '--offline', str(source_dir / 'rust')], stdout=config_file, check=True)
    shutil.copyfile(Path(__file__).with_name('windows-runtime.json'), source_dir / 'windows-runtime.json')
    (source_dir / 'README.txt').write_text(
        'Exact MSYS2 source packages include upstream sources, patches and PKGBUILD build recipes.\n'
        'Unpack each .src.tar.zst with tar and follow its PKGBUILD using MSYS2 UCRT64.\n'
        'Rust sources are under rust/; update the directory in cargo-config.toml before reuse.\n'
        'DLDR source and application build instructions accompany this archive.\n', encoding='utf-8')
    runtime_source = output / f'DLDR_{version}_windows-runtime-sources.tar.gz'
    with tarfile.open(runtime_source, 'w:gz', compresslevel=6) as package:
        package.add(source_dir, arcname='runtime-sources')
    shutil.rmtree(source_dir)
    artifacts = [zip_path, output / f'DLDR_{version}_windows-source.tar.gz', runtime_source]
    artifacts.append(sign(zip_path, version))
    if not args.no_installer:
        installer = output / f'DLDR_{version}_x64-setup.exe'
        artifacts.extend([installer, sign(installer, version)])
    (output / 'SHA256SUMS.txt').write_text(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.name}\n' for p in artifacts))
    print(f'Windows release: {zip_path}\nExecutable: {stage / "DLDR.Desktop.exe"}')


if __name__ == '__main__':
    main()
