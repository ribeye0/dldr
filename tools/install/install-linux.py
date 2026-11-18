#!/usr/bin/env python3
"""Install downloaded DLDR release files for the current user; never touches profiles."""
import argparse
import os
from pathlib import Path
import shutil

MARKER = "DLDR managed installation v1\n"


def roots():
    data = Path(os.environ.get("XDG_DATA_HOME") or Path.home() / ".local/share")
    return data, Path.home() / ".local/bin"


def desktop_quote(path):
    text = str(path).replace("%", "%%")
    for char in ("\\", '"', "`", "$"):
        text = text.replace(char, "\\" + char)
    return ('"' + text + '"').replace("\\", "\\\\")


def install(data, commands, appimage=None, cli=None, uninstall=False):
    data, commands = Path(data).absolute(), Path(commands).absolute()
    app = data / "dldr"
    marker = app / ".managed-install"
    entry = data / "applications/io.github.ribeye0.dldr.desktop"
    legacy_entry = data / "applications/dldr.desktop"
    command = commands / "dldr"
    if app.is_symlink() or (app.exists() and (not marker.is_file() or marker.read_text() != MARKER)):
        raise ValueError(f"Refusing to replace an unrelated directory: {app}")
    owned_command = command.is_symlink() and command.readlink() == app / "bin/dldr"
    if not uninstall and (command.exists() or command.is_symlink()) and not owned_command:
        raise ValueError(f"Command already exists: {command}. Remove or relocate it before installing.")
    owned_entry = entry.is_file() and f"X-DLDR-Managed={app}\n" in entry.read_text()
    if entry.exists() and not owned_entry:
        raise ValueError(f"Refusing to replace an unrelated desktop entry: {entry}")
    owned_legacy_entry = legacy_entry.is_file() and f"X-DLDR-Managed={app}\n" in legacy_entry.read_text()
    if uninstall:
        if owned_command:
            command.unlink()
        if owned_entry:
            entry.unlink()
        if owned_legacy_entry:
            legacy_entry.unlink()
        if marker.is_file():
            shutil.rmtree(app)
        return
    files = [(Path(p).absolute(), target) for p, target in [(appimage, app / "DLDR.AppImage"), (cli, app / "bin/dldr")] if p]
    if not files:
        raise ValueError("Provide --appimage FILE and/or --cli FILE.")
    for source, _ in files:
        if not source.is_file():
            raise ValueError(f"File not found: {source}")
    app.mkdir(parents=True, exist_ok=True)
    marker.write_text(MARKER)
    for source, target in files:
        target.parent.mkdir(parents=True, exist_ok=True)
        temporary = target.with_name(target.name + ".new")
        shutil.copyfile(source, temporary)
        temporary.chmod(0o755)
        temporary.replace(target)
    if (app / "bin/dldr").is_file():
        commands.mkdir(parents=True, exist_ok=True)
        if owned_command:
            command.unlink()
        command.symlink_to(app / "bin/dldr")
    if (app / "DLDR.AppImage").is_file():
        icon = Path(__file__).with_name("icon.png")
        if icon.is_file():
            shutil.copyfile(icon, app / "icon.png")
        icon_value = str(app / "icon.png").replace("\\", "\\\\") if (app / "icon.png").is_file() else "folder-download"
        entry.parent.mkdir(parents=True, exist_ok=True)
        entry.write_text("[Desktop Entry]\nType=Application\nName=DLDR\nGenericName=DLsite Downloader\n"
                         f"Exec={desktop_quote(app / 'DLDR.AppImage')}\nIcon={icon_value}\n"
                         f"Terminal=false\nCategories=AudioVideo;Audio;\nStartupWMClass=io.github.ribeye0.dldr\nX-DLDR-Managed={app}\n")
        if owned_legacy_entry:
            legacy_entry.unlink()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--appimage", type=Path)
    parser.add_argument("--cli", type=Path)
    parser.add_argument("--uninstall", action="store_true", help="Remove this install; keep library, accounts and downloaded works")
    args = parser.parse_args()
    if args.uninstall and (args.appimage or args.cli):
        parser.error("--uninstall cannot be combined with installation files")
    try:
        data, commands = roots()
        install(data, commands, args.appimage, args.cli, args.uninstall)
    except (OSError, ValueError) as error:
        parser.exit(1, f"{error}\n")
    if args.uninstall:
        print("DLDR program files removed. Library, accounts and downloaded works were kept.")
    else:
        print(f"Installed to {data / 'dldr'}. Add {commands} to PATH for dldr.")


if __name__ == "__main__":
    main()
