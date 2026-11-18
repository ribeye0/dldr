#!/usr/bin/env bash
# Build a native release and install this user's personal Linux launchers.
set -euo pipefail

if [[ $(uname -s) != Linux ]]; then
  echo 'This installer supports Linux only.' >&2
  exit 1
fi
case ${1:-} in
  '') build=true ;;
  --skip-build) build=false ;;
  *) echo "Usage: $0 [--skip-build]" >&2; exit 2 ;;
esac

project_root=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)
cd -- "$project_root"
if "$build"; then
  pnpm --filter @dldr/desktop exec tauri build --no-bundle
  cargo build --release --package dldr-cli
fi

target_dir=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
binary="$target_dir/release/dldr-desktop"
cli_binary="$target_dir/release/dldr"
if [[ ! -x $binary ]]; then
  echo "Native release binary not found: $binary" >&2
  exit 1
fi
if [[ ! -x $cli_binary ]]; then
  echo "CLI release binary not found: $cli_binary" >&2
  exit 1
fi

export DLDR_PERSONAL_SOURCE="$project_root" DLDR_PERSONAL_BINARY="$binary" DLDR_PERSONAL_CLI="$cli_binary"
python3 - <<'PY'
import os
from pathlib import Path
import shlex
import shutil
import subprocess

def desktop_value(value):
    return str(value).replace('\\', '\\\\').replace('\n', '\\n').replace('\r', '\\r')

def desktop_exec(path):
    # Exec quoting is distinct from shell quoting and desktop string escaping.
    value = str(path).replace('%', '%%')
    for char in ('\\', '"', '`', '$'):
        value = value.replace(char, '\\' + char)
    return desktop_value('"' + value + '"')


source = Path(os.environ['DLDR_PERSONAL_SOURCE'])
data = Path(os.environ.get('XDG_DATA_HOME') or Path.home() / '.local/share')
app = data / 'dldr-personal'
applications = data / 'applications'
commands = Path.home() / '.local/bin'
for directory in (app / 'bin', applications, commands):
    directory.mkdir(parents=True, exist_ok=True)

cli_command = commands / 'dldr'
if cli_command.exists() or cli_command.is_symlink():
    if not cli_command.is_symlink() or cli_command.resolve() != app / 'bin/dldr':
        raise SystemExit(f'Refusing to replace an unrelated command: {cli_command}')

menu = applications / 'io.github.ribeye0.dldr.desktop'
launcher = commands / 'dldr-gui'
if menu.exists() and f'Exec={desktop_exec(launcher)}\n' not in menu.read_text():
    raise SystemExit(f'Refusing to replace an unrelated desktop entry: {menu}')

# Atomic replacement keeps an already-running version intact during upgrades.
temporary = app / 'bin/dldr-desktop.new'
shutil.copyfile(os.environ['DLDR_PERSONAL_BINARY'], temporary)
temporary.chmod(0o755)
temporary.replace(app / 'bin/dldr-desktop')
cli_temporary = app / 'bin/dldr.new'
shutil.copyfile(os.environ['DLDR_PERSONAL_CLI'], cli_temporary)
cli_temporary.chmod(0o755)
cli_temporary.replace(app / 'bin/dldr')
if cli_command.is_symlink():
    cli_command.unlink()
cli_command.symlink_to(app / 'bin/dldr')
shutil.copyfile(source / 'apps/desktop/src-tauri/icons/128x128.png', app / 'icon.png')
licenses = app / 'licenses'
licenses.mkdir(exist_ok=True)
shutil.copyfile(source / 'apps/desktop/static/fonts/OFL.txt', licenses / 'NotoSansCJK-OFL.txt')
shutil.copyfile(source / 'LICENSE.md', licenses / 'DLDR-MIT.txt')

launcher.write_text(
    '#!/usr/bin/env bash\nset -euo pipefail\n'
    f'app={shlex.quote(str(app))}\n'
    'exec flock --close --nonblock --conflict-exit-code 0 "$app/instance.lock" '
    '"$app/bin/dldr-desktop" "$@"\n'
)
launcher.chmod(0o755)


entry = (
    '[Desktop Entry]\nType=Application\nVersion=1.0\n'
    'Name=DLDR\nGenericName=DLsite Downloader\n'
    'Comment=DLsite Downloader — browse and download your purchased DLsite works\n'
    f'Exec={desktop_exec(launcher)}\n'
    f'Icon={desktop_value(app / "icon.png")}\n'
    'Terminal=false\nCategories=AudioVideo;Audio;\nStartupNotify=false\n'
    'StartupWMClass=io.github.ribeye0.dldr\n'
)
menu.write_text(entry)
menu.chmod(0o755)
if shutil.which('desktop-file-validate'):
    subprocess.run(['desktop-file-validate', str(menu)], check=True)
if shutil.which('update-desktop-database'):
    subprocess.run(['update-desktop-database', str(applications)], check=True)

desktop = None
if shutil.which('xdg-user-dir'):
    result = subprocess.run(['xdg-user-dir', 'DESKTOP'], capture_output=True, text=True)
    if result.returncode == 0 and result.stdout.strip():
        desktop = Path(result.stdout.strip())
if desktop is None and (Path.home() / 'Desktop').is_dir():
    desktop = Path.home() / 'Desktop'
if desktop is not None and desktop != Path.home() and desktop.is_dir():
    shortcut = desktop / 'DLDR.desktop'
    if shortcut.exists() and f'Exec={desktop_exec(launcher)}\n' not in shortcut.read_text():
        raise SystemExit(f'Refusing to replace an unrelated desktop shortcut: {shortcut}')
    shortcut.write_text(entry)
    shortcut.chmod(0o755)
    if shutil.which('gio'):
        subprocess.run(['gio', 'set', str(shortcut), 'metadata::trusted', 'true'], capture_output=True)
    print(f'Desktop shortcut: {shortcut}')
print(f'Application menu: {menu}')
print(f'Launch command: {launcher}')
print(f'CLI command: {cli_command}')
print(f'Installed binary: {app / "bin/dldr-desktop"}')
PY
