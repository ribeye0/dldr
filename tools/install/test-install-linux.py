import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("installer", Path(__file__).with_name("install-linux.py"))
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallationTests(unittest.TestCase):
    def test_install_update_uninstall_preserves_profile_and_works(self):
        with tempfile.TemporaryDirectory(prefix="DLDR 中文 ") as root:
            root = Path(root)
            data, commands = root / "data", root / "bin"
            profile = root / "config/dldr"
            profile.mkdir(parents=True)
            (profile / "fixture.sqlite").write_text("keep")
            image, cli = root / "source.AppImage", root / "source-cli"
            image.write_text("old GUI"); cli.write_text("CLI")
            installer.install(data, commands, image, cli)
            self.assertEqual((commands / "dldr").read_text(), "CLI")
            entry = data / "applications/io.github.ribeye0.dldr.desktop"
            self.assertIn("StartupWMClass=io.github.ribeye0.dldr\n", entry.read_text())
            # Migrate only a legacy menu entry owned by this installation.
            legacy = entry.with_name("dldr.desktop")
            legacy.write_text(entry.read_text())
            image.write_text("new GUI")
            installer.install(data, commands, image, cli)
            self.assertEqual((data / "dldr/DLDR.AppImage").read_text(), "new GUI")
            self.assertFalse(legacy.exists())
            installer.install(data, commands, uninstall=True)
            self.assertFalse((commands / "dldr").exists())
            self.assertFalse((data / "dldr").exists())
            self.assertEqual((profile / "fixture.sqlite").read_text(), "keep")

    def test_refuses_unrelated_files_and_symlink_directory(self):
        with tempfile.TemporaryDirectory() as root:
            root = Path(root); data = root / "data"; commands = root / "bin"
            data.mkdir(); commands.mkdir(); source = root / "source"; source.write_text("fixture")
            (commands / "dldr").write_text("unrelated")
            with self.assertRaises(ValueError): installer.install(data, commands, cli=source)
            self.assertFalse((data / "dldr").exists())
            (commands / "dldr").unlink()
            (data / "dldr").symlink_to(root, target_is_directory=True)
            with self.assertRaises(ValueError): installer.install(data, commands, uninstall=True)


if __name__ == "__main__":
    unittest.main()
