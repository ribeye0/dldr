#!/usr/bin/env python3
"""Publish three installer assets; keep dependency sources on a separate archive page."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import zipfile
from updater import manifest

SOURCE_TAG = 'dependency-sources'


def request(url):
    req = urllib.request.Request(url, headers={
        'User-Agent': 'DLDR-release', 'Accept': 'application/vnd.github+json',
        'X-GitHub-Api-Version': '2022-11-28',
    })
    # This header is deliberately omitted when redirecting to blob storage.
    req.add_unredirected_header('Authorization', 'Bearer ' + os.environ['GH_TOKEN'])
    return urllib.request.urlopen(req, timeout=60)


def api(repo, path, missing_ok=False):
    try:
        with request(f'https://api.github.com/repos/{repo}{path}') as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        if missing_ok and error.code == 404:
            return None
        raise


def digest(path):
    with path.open('rb') as source:
        return hashlib.file_digest(source, 'sha256').hexdigest()


def download(artifact, destination):
    expected = artifact['digest']
    if not re.fullmatch(r'sha256:[0-9a-f]{64}', expected):
        raise RuntimeError('Missing artifact digest')
    for attempt in range(1, 4):
        temporary = destination.with_suffix('.partial')
        started = time.monotonic()
        try:
            size, report_at = 0, 64 * 1024 * 1024
            checksum = hashlib.sha256()
            with request(artifact['archive_download_url']) as response, temporary.open('wb') as output:
                while chunk := response.read(1024 * 1024):
                    if time.monotonic() - started > 600:
                        raise TimeoutError('Artifact exceeded ten-minute attempt limit')
                    output.write(chunk)
                    checksum.update(chunk)
                    size += len(chunk)
                    if size >= report_at:
                        print(f"{artifact['name']}: {size // (1024 * 1024)} MiB received", flush=True)
                        report_at += 64 * 1024 * 1024
            if size != artifact['size_in_bytes'] or 'sha256:' + checksum.hexdigest() != expected:
                raise RuntimeError('Artifact size or digest mismatch')
            temporary.replace(destination)
            return
        except (OSError, RuntimeError, urllib.error.URLError) as error:
            temporary.unlink(missing_ok=True)
            detail = f'HTTP {error.code}' if isinstance(error, urllib.error.HTTPError) else type(error).__name__
            print(f"{artifact['name']}: attempt {attempt} failed ({detail})", flush=True)
            if attempt == 3:
                raise RuntimeError('Artifact download failed after three bounded attempts') from None
            time.sleep(5)


def unpack(archive, folder):
    folder.mkdir(parents=True)
    with zipfile.ZipFile(archive) as zipped:
        names = set()
        for entry in zipped.infolist():
            name = entry.filename
            if name in names or Path(name).name != name or '\\' in name or name in ('.', '..') or entry.is_dir():
                raise RuntimeError('Unexpected artifact ZIP entry')
            names.add(name)
            with zipped.open(entry) as source, (folder / name).open('wb') as target:
                shutil.copyfileobj(source, target, length=1024 * 1024)
    checksums = {}
    for line in (folder / 'SHA256SUMS.txt').read_text(encoding='utf-8').splitlines():
        match = re.fullmatch(r'([0-9a-f]{64})  (.+)', line)
        if not match:
            raise RuntimeError('Invalid checksum manifest')
        expected, name = match.groups()
        if Path(name).name != name or '\\' in name or name in checksums or name == 'SHA256SUMS.txt':
            raise RuntimeError('Invalid checksum entry')
        if digest(folder / name) != expected:
            raise RuntimeError(f'Checksum mismatch: {name}')
        checksums[name] = expected
    if set(checksums) != names - {'SHA256SUMS.txt'}:
        raise RuntimeError('Unlisted artifact files')
    return checksums


def gh(*args):
    subprocess.run(['gh', 'release', *map(str, args)], check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('tag')
    parser.add_argument('commit')
    parser.add_argument('--run', type=int, default=os.environ.get('GITHUB_RUN_ID'))
    args = parser.parse_args()
    repo = os.environ['GITHUB_REPOSITORY']
    if not re.fullmatch(r'[\w.-]+/[\w.-]+', repo) or not args.run:
        parser.error('A repository and build run are required')
    if not re.fullmatch(r'v\d+\.\d+\.\d+(?:-rc\.\d+)?', args.tag) or not re.fullmatch(r'[0-9a-f]{40}', args.commit):
        parser.error('Invalid tag or commit')
    os.environ['GH_REPO'] = repo
    output = Path('target/dist').resolve()
    output.mkdir(parents=True, exist_ok=True)
    os.chdir(tempfile.mkdtemp(prefix='publish-', dir=output))
    version = args.tag[1:]
    tag = api(repo, '/git/ref/tags/' + args.tag)['object']
    if tag['type'] == 'tag':
        tag = api(repo, '/git/tags/' + tag['sha'])['object']
    if tag['type'] != 'commit' or tag['sha'] != args.commit:
        raise RuntimeError('The release tag changed')
    jobs = api(repo, f'/actions/runs/{args.run}/jobs?filter=all&per_page=100')['jobs']
    for name in ('prepare', 'linux', 'windows'):
        matches = [job for job in jobs if job['name'] == name]
        if not matches or max(matches, key=lambda job: job['id'])['conclusion'] != 'success':
            raise RuntimeError(f'Required platform build did not pass: {name}')

    artifacts = api(repo, f'/actions/runs/{args.run}/artifacts?per_page=100')['artifacts']
    files, hashes = {}, {}
    for platform in ('linux', 'windows'):
        matches = [a for a in artifacts if a['name'] == f'dldr-{platform}-x86_64']
        if len(matches) != 1 or matches[0]['expired']:
            raise RuntimeError(f'Missing {platform} artifact')
        archive = Path(f'{platform}.zip')
        download(matches[0], archive)
        folder = Path('artifacts') / platform
        checksums = unpack(archive, folder)
        archive.unlink()
        if set(files) & set(checksums):
            raise RuntimeError('Duplicate platform asset')
        files.update({name: folder / name for name in checksums})
        hashes.update(checksums)

    info = json.loads(files['build-info.json'].read_text(encoding='utf-8'))
    if info['commit'] != args.commit or info['dirty']:
        raise RuntimeError('Linux build does not match a clean release checkout')
    portable = files[f'DLDR_{version}_windows_x64.zip']
    with zipfile.ZipFile(portable) as zipped:
        info = json.loads(zipped.read('DLDR/build-info.json'))
    if info['commit'] != args.commit or info['dirty'] or info['version'] != version:
        raise RuntimeError('Windows build does not match a clean release checkout')

    installers = [f'DLDR_{version}_windows_x64.zip', f'DLDR_{version}_x64-setup.exe', f'DLDR_{version}_amd64.AppImage']
    if not set(installers) <= set(files):
        raise RuntimeError('Missing installer assets')
    index = Path('latest.json')
    index.write_text(json.dumps(manifest(version, files, repo), indent=2) + '\n', encoding='utf-8')
    files[index.name] = index
    hashes[index.name] = digest(index)
    release_assets = [*installers, index.name]
    sources = sorted(name for name in files if re.fullmatch(
        rf'DLDR_{re.escape(version)}_(?:windows-runtime-sources\.tar\.gz|runtime-sources\.tar\.zst(?:\.part-\d+)?)', name))
    if not sources or not any('windows-runtime-sources' in n for n in sources) or not any('_runtime-sources' in n for n in sources):
        raise RuntimeError('Both platforms require corresponding source archives')
    existing = api(repo, '/releases/tags/' + args.tag, missing_ok=True)
    if existing and not existing['draft']:
        raise RuntimeError('Refusing to overwrite a published release')
    source_release = api(repo, '/releases/tags/' + SOURCE_TAG, missing_ok=True)
    if source_release is None:
        gh('create', SOURCE_TAG, '--title', 'Dependency sources', '--prerelease', '--latest=false',
           '--notes', 'Corresponding third-party source archives for DLDR. Download the application from the numbered releases. Application source and build scripts are available through their version tags.')
        source_release = api(repo, '/releases/tags/' + SOURCE_TAG)
    available = {asset['name']: asset for asset in source_release['assets']}
    for name in sources:
        if name in available:
            if available[name].get('digest') != 'sha256:' + hashes[name] or available[name]['state'] != 'uploaded':
                raise RuntimeError(f'Existing source archive differs: {name}')
        else:
            gh('upload', SOURCE_TAG, files[name])
    archived = {asset['name']: asset for asset in api(repo, '/releases/tags/' + SOURCE_TAG)['assets']}
    for name in sources:
        if archived[name]['state'] != 'uploaded' or archived[name].get('digest') != 'sha256:' + hashes[name]:
            raise RuntimeError(f'Source archive checksum mismatch: {name}')

    notes = Path('release-notes.md')
    notes.write_text(
        'A desktop library, downloader and audio player for your purchased DLsite works.\n\n'
        '- **Windows:** ZIP for portable use, EXE to install.\n'
        '- **Linux:** AppImage (glibc 2.39+).\n\n'
        'Windows builds are unsigned.\n\n'
        f'[Third-party sources](https://github.com/{repo}/releases/tag/{SOURCE_TAG})\n', encoding='utf-8')
    if existing is None:
        gh('create', args.tag, '--verify-tag', '--draft', '--prerelease=' + str('-rc.' in args.tag).lower(),
           '--title', f'DLDR {version}', '--notes-file', notes)
    if existing and set(asset['name'] for asset in existing['assets']) - set(release_assets):
        raise RuntimeError('Unexpected assets in the draft release')
    gh('upload', args.tag, *(files[name] for name in release_assets), '--clobber')
    if '-rc.' in args.tag:
        print('Prepared draft prerelease:', args.tag, flush=True)
        return
    gh('edit', args.tag, '--draft=false', '--prerelease=' + str('-rc.' in args.tag).lower(), '--latest=' + str('-rc.' not in args.tag).lower(), '--notes-file', notes)
    published = api(repo, '/releases/tags/' + args.tag)
    assets = {asset['name']: asset for asset in published['assets']}
    if published['draft'] or set(assets) != set(release_assets):
        raise RuntimeError('Published release has an unexpected asset list')
    for name in release_assets:
        if assets[name]['state'] != 'uploaded' or assets[name].get('digest') != 'sha256:' + hashes[name]:
            raise RuntimeError(f'Published checksum mismatch: {name}')
    print('Published three verified packages and update index:', published['html_url'], flush=True)


if __name__ == '__main__':
    main()
