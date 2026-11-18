# Releases

DLDR publishes an x86_64 AppImage and a standalone `dldr` archive, built on
Ubuntu 24.04 (glibc 2.39+). Updates are manual through
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
  python3 python3-requests dpkg-dev xz-utils zstd xvfb xauth webkit2gtk-driver dbus-x11
pnpm install --frozen-lockfile
cargo fetch --locked
python3 tools/release/validate.py
pnpm check
pnpm test
python3 tools/install/test-install-linux.py
cargo fmt --all --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace
bash tools/release/scan-secrets.sh
bash tools/release/build-linux.sh
dbus-run-session -- xvfb-run -a python3 tools/release/smoke-native.py target/dist/release
```

Artifacts are written to `target/dist/release/`: AppImage, CLI, licenses/notices, DLDR
source, corresponding runtime sources, installation/release notes, build metadata
and SHA256 checksums. Runtime sources may be split to fit GitHub's file limit.
Allow several GB of disk space for the build, source downloads and Cargo vendor.

`tools/release/tools.json` pins bundler/runtime inputs and hashes. The build
bundles libmpv, WebKit media dependencies and CJK fonts, excludes the host-supplied
Wayland client library, records actual package versions and repacks before hashing.
Missing notices or exact matching sources fail the build. See
[third-party distribution details](THIRD-PARTY.md).

The native smoke check uses temporary profiles and synthetic media. It verifies
packaged codecs, four interface languages, themes, IPC, the update link and CLI
startup/login protection. It does not validate an authenticated DLsite session
or physical audio output.

After a successful unchanged GUI/CLI build, packaging-only retries may use
`bash tools/release/build-linux.sh --reuse-build`. Rebuild normally after
application or runtime dependency changes. CI always performs the full build.

## Publish a release

1. Update the workspace version in `Cargo.toml`, root and desktop `package.json`,
   `Cargo.lock` and `apps/desktop/src-tauri/tauri.conf.json` to the same version.
   Rust members inherit workspace metadata; do not duplicate their versions.
2. Run the checks above, review the diff and commit to `main`.
3. Create and push a matching `vVERSION` tag. Existing published tags are immutable.
4. Check the release workflow in Actions. It validates versions and `main`
   ancestry, builds/tests both platforms and uploads verified assets.
   Stable tags publish a latest Release;
   `-rc` tags remain draft prereleases.

The build generates a short `RELEASE-NOTES.md` with artifact names, requirements
and current limits. Review its contents before pushing the tag; release
descriptions belong on
GitHub rather than in dated development notes.

The repository-scoped `GITHUB_TOKEN` is sufficient; no updater/signing key is
needed. Workflow dispatch accepts an existing tag for retries. Re-runs may replace
assets on drafts and refuse to overwrite published releases. The release job
publishes stable versions only after both platform build/check jobs succeed.

The desktop crate emits only its Rust library and executable. CI removes unused
runner SDKs and validation build outputs before packaging to leave space for
AppImage runtime sources. These cleanup steps apply only to disposable runners.

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
