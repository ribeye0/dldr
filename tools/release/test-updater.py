#!/usr/bin/env python3
"""Verify real Tauri signatures, tamper rejection and all update targets locally."""
import base64
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch
import updater


class UpdaterTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.folder = tempfile.TemporaryDirectory(prefix='dldr-update-signatures-')
        cls.root = Path(cls.folder.name)
        cls.key = cls.root / 'test.key'
        result = subprocess.run(['pnpm', 'tauri', 'signer', 'generate', '--ci', '-p', '', '-w', str(cls.key)],
                                cwd=updater.ROOT, capture_output=True)
        if result.returncode:
            raise RuntimeError('Could not generate the disposable test key')
        cls.public = Path(str(cls.key) + '.pub').read_text().strip()

    @classmethod
    def tearDownClass(cls):
        cls.folder.cleanup()

    def signed(self, name, data=b'update payload'):
        path = self.root / name; path.write_bytes(data)
        with patch.dict(os.environ, {'TAURI_SIGNING_PRIVATE_KEY':str(self.key), 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD':''}):
            updater.sign(path, '0.2.0')
        return path

    def test_signature_binds_payload_trusted_comment_key_and_version(self):
        path = self.signed('probe')
        updater.verified_signature(path, '0.2.0', self.public)
        with self.assertRaises(RuntimeError): updater.verified_signature(path, '0.3.0', self.public)

        with self.assertRaises(RuntimeError): updater.verified_signature(path, '0.2.0')
        path.write_bytes(b'modified payload')
        with self.assertRaises(RuntimeError): updater.verified_signature(path, '0.2.0', self.public)
        path = self.signed('probe')
        signature = Path(str(path) + '.sig')
        decoded = base64.b64decode(signature.read_text()).decode().replace('version:0.2.0', 'version:0.3.0')
        signature.write_text(base64.b64encode(decoded.encode()).decode())
        with self.assertRaises(RuntimeError): updater.verified_signature(path, '0.3.0', self.public)

    def test_preflight_rejects_a_different_private_key_before_building(self):
        with patch.dict(os.environ, {'TAURI_SIGNING_PRIVATE_KEY':str(self.key), 'TAURI_SIGNING_PRIVATE_KEY_PASSWORD':''}):
            with self.assertRaisesRegex(RuntimeError, 'does not match'):
                updater.check_key()

    def test_manifest_uses_the_same_three_packages_and_embeds_verified_signatures(self):
        files = {name:self.signed(name) for name in ['DLDR_0.2.0_amd64.AppImage', 'DLDR_0.2.0_x64-setup.exe', 'DLDR_0.2.0_windows_x64.zip']}
        verify = updater.verified_signature
        with patch.object(updater, 'verified_signature', side_effect=lambda path,version:verify(path,version,self.public)):
            feed = updater.manifest('0.2.0', files)
            self.assertEqual(set(feed['platforms']), {'linux-x86_64','windows-x86_64','windows-portable-x86_64'})
            for value in feed['platforms'].values():
                self.assertTrue(value['url'].startswith('https://github.com/ribeye0/dldr/releases/download/v0.2.0/'))
                self.assertIn('trusted comment:', base64.b64decode(value['signature']).decode())
            files['DLDR_0.2.0_windows_x64.zip'].write_bytes(b'damaged ZIP')
            with self.assertRaises(RuntimeError): updater.manifest('0.2.0', files)


if __name__ == '__main__':
    unittest.main()
