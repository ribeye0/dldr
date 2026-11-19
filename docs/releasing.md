# Releases

DLDR publishes an x86_64 AppImage, built on
Ubuntu 24.04 (glibc 2.39+). Signed updates are served through
[GitHub Releases](https://github.com/ribeye0/dldr/releases). Windows 11 x64 releases include a portable ZIP and a per-user NSIS installer.
See [installation and data compatibility](linux-install.md).

## Build and verify

Use Ubuntu 24.04 x86_64, Node.js 22, pnpm **12.3.4** and Rust **1.100.0** with
rustfmt/clippy. Enable source repositories so bundled libraries can be distributed
with their exact corresponding sources:

```sh
sudo sed -i 's/^Types: deb$/Types: deb deb-src/' /etc/apt/sources.list.d/ubuntu.sources
sudo apt-get update
sudo apt-get install --yes --no-install-recommends build-essential curl file pkg-config patchelf \
  libwebkit2gtk-4.1-dev libayatana-appindicator3-dev librsvg2-dev libxdo-dev libssl-dev \
  libmpv2 fonts-noto-cjk ffmpeg gstreamer1.0-libav gstreamer1.0-plugins-base gstreamer1.0-plugins-good \
  python3 python3-requests dpkg-dev xz-utils zstd xvfb xauth webkit2gtk-driver dbus-x11 pulseaudio
pnpm install --frozen-lockfile
cargo fetch --locked
python3 tools/release/validate.py
pnpm check
pnpm test
python3 tools/install/test-install-linux.py
python3 tools/release/test-notices.py
python3 tools/release/test-publish.py
python3 tools/release/test-updater.py
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
bash tools/release/scan-secrets.sh
bash tools/release/build-linux.sh
dbus-run-session -- xvfb-run -a python3 tools/release/smoke-native.py target/dist/release
```

Artifacts are written to `target/dist/release/`: AppImage, CLI, licenses/notices, DLDR
source, corresponding runtime sources, installation instructions, build metadata
and SHA256 checksums. Runtime sources may be split to fit GitHub's file limit.
Allow several GB of disk space for the build, source downloads and Cargo vendor.

`tools/release/tools.json` pins bundler/runtime inputs and hashes. The build
bundles libmpv, WebKit media dependencies and CJK fonts, excludes the host-supplied
Wayland client library, records actual package versions and repacks before hashing.
Missing notices or exact matching sources fail the build. See
[third-party distribution details](THIRD-PARTY.md).

The native smoke check uses temporary profiles and synthetic media. It verifies
packaged codecs, four interface languages, themes, IPC, the update dialog and CLI
startup/login protection. It does not validate an authenticated DLsite session
or physical audio output.

After a successful unchanged GUI/CLI build, packaging-only retries may use
`bash tools/release/build-linux.sh --reuse-build`. Rebuild normally after
application or runtime dependency changes. The release workflow always performs the full build.

## Publish a release

### Update signing

Keep the updater private key outside the repository. Local builds look for
`~/.local/share/dldr-signing/updater.key`, or use `TAURI_SIGNING_PRIVATE_KEY`
(key contents or an absolute file path). Set `TAURI_SIGNING_PRIVATE_KEY_PASSWORD`
for an encrypted key. Back up this key securely: existing installations trust
the public key embedded in `tauri.conf.json` and cannot accept a replacement key.

For Actions packaging, set repository secrets `TAURI_SIGNING_PRIVATE_KEY` (the
key contents) and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (if needed). No private
key belongs in Git, source archives or release assets.

The Linux build signs **after** the final AppImage repack. The Windows build
signs both the final installer and portable ZIP. The publisher verifies all
three signatures and embeds them in `latest.json`; separate `.sig` attachments
are unnecessary. Stable releases use `releases/latest/download/latest.json`;
prereleases do not become the stable update channel.

For a manual release, collect the three packages and their `.sig` files into
one directory, then run (OpenSSL is needed for signature verification):

```sh
python3 tools/release/updater.py manifest target/dist/release --version VERSION
```

Upload the three signed packages, the Arch package and `latest.json` to the **same** stable release.
Publish only after both platforms are ready. Do not upload the private key or
the separate signatures. Existing releases without the updater need one manual
upgrade before in-app updating becomes available.

### Publish

Automatic CI tests are disabled; run the checks above locally. The release
workflow only packages and publishes files. A release can also be created manually by uploading
locally built and verified installers through GitHub's Releases page or `gh`.

1. Update the workspace, lockfile and desktop/frontend versions together.
2. Check the changes and commit to `main`.
3. Build and verify packages locally, then create and push `vVERSION`. Tag pushes do not start CI.
4. Upload the packages and `latest.json` on GitHub Releases. **DLDR release** is an optional, manually started workflow for AppImage and Windows builds; it does not build the Arch package.
5. The stable release contains:
   - `DLDR_VERSION_windows_x64.zip`
   - `DLDR_VERSION_x64-setup.exe`
   - `DLDR_VERSION_amd64.AppImage`
   - `dldr-VERSION-1-x86_64.pkg.tar.zst`
   - `latest.json` (small machine-readable update index)

Release notes contain a short description, platform requirements and a source
archive link. GitHub also adds its own repository source ZIP/tarball links.

Corresponding dependency sources are kept on the separate
[Dependency sources](https://github.com/ribeye0/dldr/releases/tag/dependency-sources)
archive page. Versioned archive files are verified and retained. Licenses remain
inside the application packages. Source availability is verified before the
installers are published. Build metadata, test reports, standalone CLI builds and
checksum manifests remain in Actions artifacts rather than the user-facing release.

`tools/release/publish.py` streams artifacts through the REST API with bounded
retries, verifies their SHA256 digests and manifests, and checks both platforms'
embedded commit metadata. Only the publisher has repository write permissions.
The workflow caches dependencies and downloaded source packages. Existing published
versions are never overwritten; `-rc` tags remain draft prereleases.

The desktop crate emits only its Rust library and executable. CI removes unused
runner SDKs before AppImage packaging. These cleanup steps apply only to runners.

## Before tagging a stable version

Verify a release candidate or local build with `SHA256SUMS.txt` and confirm
`build-info.json` identifies a clean checkout of the intended tag. Complete:

- AppImage startup, audio output, Wayland/X11, links and folder opening on a clean
  supported desktop, including paths containing spaces and Chinese characters.
- Official login/MFA, online playback/subtitles and the DLsite Play download button.
- Selected-work and parallel downloads, pause/resume after restart, cancel, retry
  and shared rate limiting using explicitly chosen purchased works.
- Large-library pagination/search/selection, genres/creator credits, playlists,
  local audio/VTT and metadata backup/restore; upgrades retain existing data.
- Inspect files, screenshots, logs and source archives for private credentials,
  account databases or purchased media.

Record material limitations in the release notes. For an `-rc` version, the
workflow leaves a draft that can be published as a prerelease. Automated smoke
tests do not replace these desktop
and account checks. Repository visibility is managed separately from releases.

## Windows x64

On Windows, install the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/)
(Rust MSVC, Visual Studio C++ build tools and WebView2), Node.js 22, pnpm 12.3.4
and Python 3. Packaging also requires a `tar` with Zstandard support (provided
by current Windows 11's `tar.exe`; Linux builds need `zstd`). Then run:

```sh
pnpm install --frozen-lockfile
cargo fetch --locked
python tools/release/build-windows.py
```

The script creates `target/dist/windows/`: `DLDR/DLDR.Desktop.exe`, CLI, a portable ZIP,
NSIS installer, application-source snapshot, dependency notices and checksums.
It verifies pinned MSYS2 UCRT64 runtime and source packages, vendors Rust sources,
and places replaceable DLLs beside both installed and portable executables. `--no-installer` skips NSIS;
`--reuse-build` is only for repackaging unchanged Windows binaries.

Linux cross builds additionally require Rust's `x86_64-pc-windows-msvc` target,
LLVM/Clang **19+**, LLD, NSIS and cargo-xwin **0.23.1**. Set `XWIN_ARCH=x86_64`,
`XWIN_CACHE_DIR` to an external cache, use `cargo xwin cache xwin` to populate it,
and set `RUSTFLAGS="-C target-feature=+crt-static"`. Keep these cross-build settings
outside repository configuration. See the [Tauri cross-build guide](https://v2.tauri.app/distribute/windows-installer/).
The script copies UnRAR 0.7.7 into its build cache to fix host/target detection,
SDK header case and Clang SIMD handling; it does not modify Cargo registry files.
Native MSVC builds use the original dependency. Follow the
[Windows usage and checks](windows.md); Wine does not replace native desktop acceptance.

## Arch Linux x86_64

Build on an up-to-date Arch Linux system. Install `base-devel`, `rust`, `nodejs`,
`pnpm`, `python`, `desktop-file-utils`, `webkit2gtk-4.1`, `openssl`, `mpv`,
`libarchive` and `xdg-utils`. Then run:

```sh
pnpm install --frozen-lockfile
cargo fetch --locked
python3 tools/release/build-arch.py
```

The native package includes the GUI, CLI, desktop entry, icons and dependency
notices. Runtime libraries come from pacman; the AppImage is not embedded.
The recipe is in `tools/release/arch/PKGBUILD`. Inspect with `pacman -Qip` and
`pacman -Qlp`, then install with `sudo pacman -U FILE.pkg.tar.zst`. Update this
package with pacman rather than the AppImage/Windows updater.
