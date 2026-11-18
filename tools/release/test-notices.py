#!/usr/bin/env python3
"""Check ownership of ELF libraries relocated and patched by the bundler."""
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from notices import relocated_owners


class OwnershipTests(unittest.TestCase):
    def test_patched_library_keeps_its_owner_among_unrelated_namesakes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            libraries = []
            for name, value in [('system', 1), ('browser', 2)]:
                directory = root / name
                directory.mkdir()
                source = directory / 'library.c'
                source.write_text(f'int example(void) {{ return {value}; }}\n')
                library = directory / 'libexample.so'
                subprocess.run(['cc', '-shared', '-Wl,--build-id', str(source), '-o', str(library)], check=True)
                libraries.append(library)
            bundled = root / 'libexample.so'
            shutil.copyfile(libraries[0], bundled)
            subprocess.run(['patchelf', '--set-rpath', '$ORIGIN', str(bundled)], check=True)
            candidates = {libraries[0]: {'system-library'}, libraries[1]: {'google-chrome-stable'}}
            self.assertEqual(relocated_owners(bundled, candidates, {}), {'system-library'})
            self.assertEqual(relocated_owners(bundled, {libraries[1]: {'google-chrome-stable'}}, {}), set())
            self.assertEqual(relocated_owners(bundled, {}, {}), set())


if __name__ == '__main__':
    unittest.main()
