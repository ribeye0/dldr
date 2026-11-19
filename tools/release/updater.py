#!/usr/bin/env python3
"""Sign final release files and build the small Tauri update index."""
import argparse
import base64
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[2]


def signing_environment():
    env = os.environ.copy()
    if not env.get('TAURI_SIGNING_PRIVATE_KEY'):
        key = Path(env.get('XDG_DATA_HOME', str(Path.home() / '.local/share'))) / 'dldr-signing/updater.key'
        if not key.is_file():
            raise RuntimeError('Set TAURI_SIGNING_PRIVATE_KEY or keep your key in ~/.local/share/dldr-signing/updater.key')
        env['TAURI_SIGNING_PRIVATE_KEY'] = str(key)
    env.setdefault('TAURI_SIGNING_PRIVATE_KEY_PASSWORD', '')
    return env


def sign(path, version):
    env = signing_environment()
    key = env['TAURI_SIGNING_PRIVATE_KEY']
    if len(key) < 4096 and '\n' not in key and Path(key).is_file():
        env['TAURI_SIGNING_PRIVATE_KEY_PATH'] = key
        del env['TAURI_SIGNING_PRIVATE_KEY']
    pnpm = shutil.which('pnpm.cmd' if os.name == 'nt' else 'pnpm')
    result = subprocess.run([pnpm, 'tauri', 'signer', 'sign', '--app-version', version, str(Path(path).resolve())],
                            cwd=ROOT, env=env, capture_output=True)
    if result.returncode:
        raise RuntimeError(f'Could not sign {Path(path).name}; check the signing key and password')
    return Path(str(path) + '.sig')


def check_key():
    """Fail before a long build if the configured key is missing or different."""
    config = json.loads((ROOT / 'apps/desktop/src-tauri/tauri.conf.json').read_text())
    public = base64.b64decode(config['plugins']['updater']['pubkey'], validate=True).decode().splitlines()
    public_id = base64.b64decode(public[1], validate=True)[2:10]
    with tempfile.TemporaryDirectory(prefix='dldr-key-check-') as temporary:
        probe = Path(temporary) / 'probe'
        probe.write_bytes(b'DLDR signing key check')
        signature = sign(probe, config['version'])
        lines = base64.b64decode(signature.read_text().strip(), validate=True).decode().splitlines()
        if base64.b64decode(lines[1], validate=True)[2:10] != public_id:
            raise RuntimeError('The signing key does not match the public key in tauri.conf.json')


def verified_signature(path, version=None, public_key=None):
    """Verify both Minisign signatures with OpenSSL's Ed25519 implementation.

    Matches minisign-verify: BLAKE2b-512 prehash plus the signed trusted comment.
    This catches wrong keys, damaged bytes and damaged comments before publishing.
    """
    config = json.loads((ROOT / 'apps/desktop/src-tauri/tauri.conf.json').read_text())
    public = base64.b64decode(public_key or config['plugins']['updater']['pubkey'], validate=True).decode().splitlines()
    key = base64.b64decode(public[1], validate=True)
    encoded = Path(str(path) + '.sig').read_text().strip()
    lines = base64.b64decode(encoded, validate=True).decode().splitlines()
    signature = base64.b64decode(lines[1], validate=True)
    global_signature = base64.b64decode(lines[3], validate=True)
    if len(key) != 42 or len(signature) != 74 or len(global_signature) != 64 or signature[:2] != b'ED' \
            or key[2:10] != signature[2:10] or not lines[2].startswith('trusted comment: '):
        raise RuntimeError(f'Invalid signing key or signature: {Path(path).name}')
    if version is not None and lines[2][17:].split('\t').count(f'version:{version}') != 1:
        raise RuntimeError(f'Signed version does not match {version}: {Path(path).name}')
    checksum = hashlib.blake2b(digest_size=64)
    with Path(path).open('rb') as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b''):
            checksum.update(chunk)
    openssl = shutil.which('openssl')
    if not openssl:
        raise RuntimeError('OpenSSL is required to verify release signatures')
    with tempfile.TemporaryDirectory(prefix='dldr-verify-') as temporary:
        temporary = Path(temporary)
        public_file = temporary / 'public.der'
        # RFC 8410 SubjectPublicKeyInfo for Ed25519.
        public_file.write_bytes(bytes.fromhex('302a300506032b6570032100') + key[10:])
        for message, proof in [(checksum.digest(), signature[10:]),
                               (signature[10:] + lines[2][17:].encode(), global_signature)]:
            data, sig = temporary / 'data', temporary / 'signature'
            data.write_bytes(message); sig.write_bytes(proof)
            result = subprocess.run([openssl, 'pkeyutl', '-verify', '-pubin', '-keyform', 'DER',
                                     '-inkey', str(public_file), '-rawin', '-in', str(data), '-sigfile', str(sig)],
                                    capture_output=True)
            if result.returncode:
                raise RuntimeError(f'Update signature verification failed: {Path(path).name}')
    return encoded


def manifest(version, files, repository='ribeye0/dldr', notes=''):
    if not re.fullmatch(r'\d+\.\d+\.\d+(?:-rc\.\d+)?', version) or not re.fullmatch(r'[\w.-]+/[\w.-]+', repository):
        raise ValueError('Invalid update version or repository')
    platforms = {}
    for target, name in {
        'linux-x86_64': f'DLDR_{version}_amd64.AppImage',
        'windows-x86_64': f'DLDR_{version}_x64-setup.exe',
        'windows-portable-x86_64': f'DLDR_{version}_windows_x64.zip',
    }.items():
        platforms[target] = {
            'url': f'https://github.com/{repository}/releases/download/v{version}/{name}',
            'signature': verified_signature(files[name], version),
        }
    return {'version': version, 'notes': notes, 'pub_date': datetime.now(timezone.utc).isoformat(), 'platforms': platforms}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    commands.add_parser('check-key')
    signer = commands.add_parser('sign'); signer.add_argument('file', type=Path); signer.add_argument('--version', required=True)
    feed = commands.add_parser('manifest'); feed.add_argument('directory', type=Path); feed.add_argument('--version', required=True)
    feed.add_argument('--notes', default=''); feed.add_argument('--repository', default='ribeye0/dldr')
    args = parser.parse_args()
    if args.command == 'check-key':
        check_key()
        print('Signing key matches the configured public key')
    elif args.command == 'sign':
        sign(args.file, args.version)
        print(f'Signed {args.file.name}')
    else:
        files = {path.name: path for path in args.directory.iterdir() if path.is_file()}
        value = manifest(args.version, files, args.repository, args.notes)
        output = args.directory / 'latest.json'
        output.write_text(json.dumps(value, indent=2) + '\n')
        print(f'Update index: {output}')


if __name__ == '__main__':
    main()
