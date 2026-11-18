#!/usr/bin/env python3
"""Check installer selection, source availability and rejection of corrupt artifacts."""
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.request
import zipfile

spec = importlib.util.spec_from_file_location('publisher', Path(__file__).with_name('publish.py'))
publisher = importlib.util.module_from_spec(spec)
spec.loader.exec_module(publisher)
COMMIT = 'a' * 40


def zipped(files):
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, 'w') as archive:
        for name, data in files.items():
            archive.writestr(name, data)
    return stream.getvalue()


class PublishTests(unittest.TestCase):
    def test_publish_only_installers_after_source_archiving(self):
        version = '0.1.1'
        info = json.dumps({'commit': COMMIT, 'dirty': False, 'version': version}).encode()
        contents = {
            'linux': {'build-info.json': info, f'DLDR_{version}_amd64.AppImage': b'linux',
                      f'DLDR_{version}_runtime-sources.tar.zst': b'linux-sources', 'acceptance.json': b'{}'},
            'windows': {f'DLDR_{version}_windows_x64.zip': zipped({'DLDR/build-info.json': info}),
                        f'DLDR_{version}_x64-setup.exe': b'installer',
                        f'DLDR_{version}_windows-runtime-sources.tar.gz': b'windows-sources'},
        }
        blobs, artifacts = {}, []
        for platform, files in contents.items():
            sums = ''.join(f'{hashlib.sha256(value).hexdigest()}  {name}\n' for name, value in files.items()).encode()
            blob = zipped({**files, 'SHA256SUMS.txt': sums})
            url = 'https://api.github.com/' + platform
            blobs[url] = blob
            artifacts.append({'name': f'dldr-{platform}-x86_64', 'expired': False,
                              'archive_download_url': url, 'size_in_bytes': len(blob),
                              'digest': 'sha256:' + hashlib.sha256(blob).hexdigest()})
        releases = {}

        def api(repo, path, missing_ok=False):
            if path == '/git/ref/tags/v0.1.1':
                return {'object': {'type': 'commit', 'sha': COMMIT}}
            if '/jobs?' in path:
                return {'jobs': [{'id': index, 'name': name, 'conclusion': 'success'}
                                 for index, name in enumerate(('prepare', 'linux', 'windows'))]}
            if '/artifacts?' in path:
                return {'artifacts': artifacts}
            if path.startswith('/releases/tags/'):
                return releases.get(path.rsplit('/', 1)[1])
            raise AssertionError(path)

        def gh(command, tag, *args):
            if command == 'create':
                self.assertNotIn('--target', args)
                releases[tag] = {'draft': '--draft' in args, 'assets': [], 'html_url': 'https://github.com/test/release'}
            elif command == 'upload':
                for name in args:
                    if str(name).startswith('--'):
                        continue
                    path = Path(name)
                    releases[tag]['assets'].append({'name': path.name, 'state': 'uploaded',
                                                   'digest': 'sha256:' + publisher.digest(path)})
            elif command == 'edit':
                self.assertEqual(len(releases[publisher.SOURCE_TAG]['assets']), 2)
                releases[tag]['draft'] = False
            else:
                raise AssertionError(command)

        previous = Path.cwd()
        with tempfile.TemporaryDirectory() as temporary, \
                patch.dict(os.environ, {'GITHUB_REPOSITORY': 'test/dldr', 'GITHUB_RUN_ID': '123'}), \
                patch('sys.argv', ['publish.py', 'v0.1.1', COMMIT]), \
                patch.object(publisher, 'api', side_effect=api), \
                patch.object(publisher, 'gh', side_effect=gh), \
                patch.object(publisher, 'request', side_effect=lambda url: io.BytesIO(blobs[url])):
            os.chdir(temporary)
            try:
                publisher.main()
                self.assertEqual({a['name'] for a in releases['v0.1.1']['assets']}, {
                    'DLDR_0.1.1_windows_x64.zip', 'DLDR_0.1.1_x64-setup.exe', 'DLDR_0.1.1_amd64.AppImage'})
            finally:
                os.chdir(previous)

    def test_corrupt_unlisted_and_unsafe_entries_are_rejected(self):
        for files in ({'../escape': b'bad'}, {'binary': b'data', 'SHA256SUMS.txt': b'0' * 64 + b'  binary\n'},
                      {'extra': b'data', 'SHA256SUMS.txt': b''}):
            with self.subTest(files=files), tempfile.TemporaryDirectory() as temporary:
                archive = Path(temporary) / 'artifact.zip'
                archive.write_bytes(zipped(files))
                with self.assertRaises(RuntimeError):
                    publisher.unpack(archive, Path(temporary) / 'output')

    def test_retry_and_redirect_credentials(self):
        blob = b'complete-download'
        artifact = {'name': 'fixture', 'digest': 'sha256:' + hashlib.sha256(blob).hexdigest(),
                    'size_in_bytes': len(blob), 'archive_download_url': 'https://api.github.com/fixture'}
        with tempfile.TemporaryDirectory() as temporary, \
                patch.object(publisher, 'request', side_effect=[TimeoutError(), io.BytesIO(blob)]), \
                patch.object(publisher.time, 'sleep'):
            target = Path(temporary) / 'artifact.zip'
            publisher.download(artifact, target)
            self.assertEqual(target.read_bytes(), blob)
        request = urllib.request.Request('https://api.github.com/fixture')
        request.add_unredirected_header('Authorization', 'Bearer fixture')
        redirected = urllib.request.HTTPRedirectHandler().redirect_request(
            request, None, 302, 'Found', {}, 'https://example.invalid/blob')
        self.assertFalse(redirected.has_header('Authorization'))


if __name__ == '__main__':
    unittest.main()
