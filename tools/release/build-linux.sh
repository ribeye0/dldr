#!/usr/bin/env bash
# Run inside Ubuntu 24.04 amd64 with the prerequisites in docs/releasing.md.
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."
source /etc/os-release
if [[ $ID != ubuntu || $VERSION_ID != 24.04 || $(uname -m) != x86_64 ]]; then
  echo 'Release builds require Ubuntu 24.04 x86_64; do not bundle host Arch libraries.' >&2
  exit 1
fi
version=$(python3 tools/release/validate.py)
reuse_build=false
if [[ ${1:-} == --reuse-build ]]; then reuse_build=true; shift; fi
target=$(cargo metadata --locked --offline --no-deps --format-version 1 | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
output=$(realpath -m "${1:-target/dist/release}")
mkdir -p "$output"
work=$(mktemp -d "$target/dldr-release.XXXXXX")
trap 'rm -rf -- "$work"' EXIT
export SOURCE_DATE_EPOCH=${SOURCE_DATE_EPOCH:-$(git log -1 --format=%ct)}
cargo fetch --locked
python3 tools/release/prepare-tools.py
export LDAI_RUNTIME_FILE="${XDG_CACHE_HOME:-$HOME/.cache}/tauri/runtime-x86_64"
if ! "$reuse_build"; then
  pnpm tauri build --bundles appimage
  cargo build --locked --release -p dldr-cli
fi
appdir="$target/release/bundle/appimage/DLDR.AppDir"
image="$target/release/bundle/appimage/DLDR_${version}_amd64.AppImage"
test -f "$image"
test -f "$appdir/usr/lib/libmpv.so.2"
test -f "$appdir/usr/lib/libarchive.so.13"
python3 tools/release/fonts.py "$appdir"
# Drivers are supplied by the host compositor. Preserve the compatibility fix.
find "$appdir/usr/lib" -maxdepth 1 -name 'libwayland-client.so.*' -delete
python3 tools/release/notices.py "$appdir" "$work"
DLDR_SOURCE_CACHE="$target/dldr-source-downloads" python3 tools/release/sources.py "$work/system-packages.json" "$work/runtime-sources"
python3 tools/release/runtime-sources.py "$work/runtime-sources"
cp "$work/system-packages.json" "$work/runtime-sources/"
# The first bundle predates our notices and Wayland fix; repack final bytes.
plugin="$work/linuxdeploy-plugin-appimage.AppImage"
curl --fail --location --silent --show-error --output "$plugin" \
  https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/1-alpha-20250213-1/linuxdeploy-plugin-appimage-x86_64.AppImage
echo "992d502a248e14ab185448ddf6f6e7d25558cb84d4623c354c3af350c25fccb3  $plugin" | sha256sum --check --strict -
chmod +x "$plugin"
rm -f -- "$image"
ARCH=x86_64 OUTPUT="$image" APPIMAGE_EXTRACT_AND_RUN=1 "$plugin" --appdir "$appdir"
cp "$image" "$output/"
tar -C "$appdir/usr/share/doc" --zstd -cf "$output/DLDR_${version}_notices.tar.zst" dldr
tar -C "$work" --zstd -cf "$output/DLDR_${version}_runtime-sources.tar.zst" runtime-sources
source_archive="$output/DLDR_${version}_runtime-sources.tar.zst"
if [[ $(stat -c %s "$source_archive") -gt 1800000000 ]]; then
  split -b 1500M -d "$source_archive" "$source_archive.part-"
  rm -- "$source_archive"
fi
mkdir "$work/cli"
cp "$target/release/dldr" tools/install/install-linux.py LICENSE.md "$work/cli/"
cp apps/desktop/src-tauri/icons/128x128.png "$work/cli/icon.png"
cp docs/linux-install.md "$work/cli/README.md"
cp -r "$appdir/usr/share/doc/dldr" "$work/cli/third-party"
tar -C "$work/cli" -czf "$output/dldr_${version}_linux_x86_64.tar.gz" .
git ls-files --cached --others --exclude-standard -z | \
  python3 -c 'import os,sys; sys.stdout.buffer.write(b"".join(name+b"\0" for name in sorted(set(sys.stdin.buffer.read().split(b"\0"))) if name and os.path.isfile(name)))' | \
  tar --null --files-from=- -czf "$output/DLDR_${version}_source.tar.gz"
cp docs/linux-install.md "$output/INSTALL.md"
cat > "$output/RELEASE-NOTES.md" <<EOF
# DLDR $version

A desktop library, downloader and audio player for purchased DLsite works.

## Downloads

- GUI: DLDR_${version}_amd64.AppImage
- CLI and optional user installer: dldr_${version}_linux_x86_64.tar.gz
- Windows 11 x64 portable: DLDR_${version}_windows_x64.zip
- Windows 11 x64 installer: DLDR_${version}_x64-setup.exe
- Verify downloads with SHA256SUMS.txt; Linux users should read INSTALL.md.
- Requires glibc 2.39+ (Ubuntu 24.04 baseline). Updates are manual.
- Windows ZIP requires Microsoft Edge WebView2; the installer offers to install it.
- Windows executables are unsigned; SmartScreen may show a warning.
- Official DLsite Play is experimental; reader-only products are not guaranteed.
- Application source, dependency notices and corresponding runtime sources accompany both platforms.

See [Linux installation](https://github.com/ribeye0/dldr/blob/main/docs/linux-install.md),
[Windows usage](https://github.com/ribeye0/dldr/blob/main/docs/windows.md) and
[CLI usage](https://github.com/ribeye0/dldr/blob/main/docs/dldr.md).
EOF
python3 - "$output" <<'PY'
import json,os,platform,subprocess,sys
from pathlib import Path
run=lambda *args:subprocess.check_output(args,text=True).strip()
value={'commit':run('git','rev-parse','HEAD'),'dirty':bool(run('git','status','--porcelain')),
       'sourceDateEpoch':os.environ['SOURCE_DATE_EPOCH'],'buildSystem':platform.platform(),
       'osRelease':Path('/etc/os-release').read_text(),'rust':run('rustc','--version'),
       'node':run('node','--version'),'pnpm':run('pnpm','--version'),
       'packages':run('dpkg-query','-W','-f=${binary:Package}\t${Version}\n')}
(Path(sys.argv[1])/'build-info.json').write_text(json.dumps(value,indent=2)+'\n')
PY
(cd "$output"; find . -maxdepth 1 -type f ! -name SHA256SUMS.txt -printf '%P\0' | sort -z | xargs -0 sha256sum > SHA256SUMS.txt; sha256sum --check SHA256SUMS.txt)
echo "Candidate artifacts: $output"
