# DLDR for Linux

Download from **https://github.com/ribeye0/dldr/releases**. The AppImage supports **x86_64 Linux with glibc 2.39 or newer**, built on Ubuntu 24.04.
macOS, ARM64, Alpine/musl and Ubuntu 22.04 are not supported by this AppImage.
Windows 11 x64 packages are documented in [Windows usage](windows.md).
Other desktop distributions must be tested; AppImage does not make every system
library or graphics driver portable.

## Arch Linux

The native Arch package includes both GUI and CLI and uses system libraries:

```sh
sudo pacman -U dldr-0.1.5-1-x86_64.pkg.tar.zst
dldr-gui
dldr --help
```

Install newer packages with `pacman -U`; in-app replacement applies only to the
AppImage. Package removal leaves your profile and downloaded works intact.

## Start the GUI

```sh
chmod +x DLDR_0.1.5_amd64.AppImage
./DLDR_0.1.5_amd64.AppImage
```

The GUI does not need Node.js, pnpm or Rust. libmpv, libarchive and the media dependencies
are packaged for the local player and official WebView. The host still provides
its desktop, graphics drivers and audio service. If FUSE is unavailable, try:

```sh
./DLDR_0.1.5_amd64.AppImage --appimage-extract-and-run
```

Open Settings → Download settings to choose the library and temporary folders;
then Settings → Accounts to sign in and sync purchased works. The lower-left
status opens download progress. Downloads never start merely by opening DLDR.

## CLI

The standalone CLI archive is a developer build artifact, not a regular release
attachment. Build it with `bash tools/release/build-linux.sh` or build just the CLI
with `cargo build --release -p dldr-cli`. If using the generated archive, extract
`dldr_0.1.5_linux_x86_64.tar.gz` into its own folder. Either run
`./dldr` there, or install the executable on your PATH:

```sh
install -Dm755 dldr "$HOME/.local/bin/dldr"
dldr login
dldr RJ01760323 --dry-run
dldr RJ01760323
dldr search '$va:小花衣こっこ$'
```

`login` prompts in a real terminal, hides passwords, supports MFA and saves an
account before syncing purchases. Passwords are not accepted in flags or stdin.
Close the GUI before CLI login/logout/sync/download/resume; cached search/show/
status can coexist. See `dldr --help` and `docs/dldr.md` in the source archive.

To launch an AppImage from the CLI:

```sh
dldr gui /absolute/path/to/DLDR_0.1.5_amd64.AppImage
```

Alternatively set `DLDR_GUI` to an executable path. It is a path, not a shell
command; spaces are supported without embedding shell arguments in its value.

## Optional desktop/menu installation

The CLI archive also contains `install-linux.py` and `icon.png`. Python 3 is
needed only for this optional installer:

```sh
python3 install-linux.py --appimage /path/to/DLDR_0.1.5_amd64.AppImage --cli ./dldr
```

It copies the program to `$XDG_DATA_HOME/dldr` (normally `~/.local/share/dldr`),
creates a DLDR application-menu entry and a `~/.local/bin/dldr` symlink.
Then `dldr gui` finds the installed AppImage automatically. Existing unrelated
commands or directories are not overwritten. If `dldr` already points to a
personal development installation, remove that symlink explicitly before using
this installer; the old program and library are retained.

Use Settings → About & updates → Check for updates to update an AppImage in
place. Its directory must be writable. The downloaded package is signature
verified in the background. You can cancel the transfer or close the dialog and keep using DLDR.
Choose **Install and restart** after it finishes; nothing is installed automatically.
**Install local package** also accepts an AppImage for upgrades or downgrades.
Unsigned self-builds need explicit trust; export a library backup before downgrading. Native development
builds and separately built CLI binaries still require rebuilding/reinstallation.
Manual replacement remains available after closing DLDR.

```sh
python3 install-linux.py --uninstall
```

This removes only this installer's program/menu files. Account data and downloaded
works remain. A manually installed CLI binary needs to be removed manually.

## Data compatibility and credentials

GUI and CLI use `~/.config/dldr/` (or `$XDG_CONFIG_HOME/dldr`) for their profile.
It contains `dldr.sqlite`, `credentials/vault.json`, `player-history.json`,
`logs/`, `webview/` and the account-scoped official-player browser profiles.
`dldr --data-dir DIR` overrides this location for CLI commands only.

Updates use the same `~/.config/dldr/` profile; keep this directory to retain your library.
Fresh profiles start with first-run setup. Earlier previews used a different
database filename: close the application before renaming that database to
`dldr.sqlite`, then export a new library backup after upgrading. Removing the
AppImage does not remove profiles or downloaded works.

Downloaded media is stored in your selected library directory. Credentials are
stored separately in a local JSON file with owner-only permissions on Linux;
they are **not encrypted or stored in the system keyring**. Use a trusted local
user account and protect its backups. `dldr logout` clears DLDR download
credentials, while the official DLsite Play window has a separate browser login;
sign out on the official site to clear that session.

DLsite Play is experimental. Use official DLsite/viviON ID login; third-party
social login is not supported. Website changes and system WebView compatibility
can affect playback. See `docs/dlsite-play.md` for current limits.

## Verify files and report problems

```sh
sha256sum --check SHA256SUMS.txt
```

Run this in the directory containing the release files. If downloading only a
subset, missing files are reported; verify the entries for the files you have.
Hashes detect changed bytes. In-app updates additionally verify the publisher signature.
Notice/source archives accompany the binaries. If the runtime source archive is
split, concatenate `.part-00`, `.part-01`, … in order before extracting with
`tar --zstd -xf`. See `THIRD-PARTY.md` in the notice archive.

Report OS/desktop, DLDR version, work ID, steps and sanitized Logs at
https://github.com/ribeye0/dldr/issues. Never attach passwords, Cookie/vault files,
your private database or purchased media.
