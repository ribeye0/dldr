# Third-party components

DLDR source is distributed under the [MIT license](../LICENSE.md). Copyright
notices for incorporated source are retained in that license. Third-party
components and bundled assets keep their respective licenses.

## Bundled interface font

DLDR embeds **Noto Sans CJK TC 2.004**, the full-coverage, language-specific
OpenType/CFF2 variable font, compressed to WOFF2. Its 100–900 weight axis covers the
interface and transcript. The complete character map and localized glyphs are
retained; this is not a subset built from today's interface strings. The HTML
language selects Traditional Chinese, Simplified Chinese or Japanese forms.
The font loads locally and is not installed into the operating system.

Copyright © 2014–2021 Adobe, with Reserved Font Name “Source”. The font retains
the SIL Open Font License 1.1 in `apps/desktop/static/fonts/OFL.txt`; distribution notices copy
it to `fonts/NotoSansCJK-OFL.txt`. The personal Linux install includes the same
text in its `licenses/` directory. AppImage's existing fontconfig fallback is
also retained for native GTK dialogs.

Source: [NotoSansCJKtc-VF.otf, upstream tag Sans2.004](https://github.com/notofonts/noto-cjk/blob/Sans2.004/Sans/Variable/OTF/NotoSansCJKtc-VF.otf).
The original OTF SHA256 is
`fb6fd590a4bcfe1b5d71b8bb0054719256b815e8fc700dd70fd171a3660a9e6b`.
The bundled `apps/desktop/static/fonts/NotoSansCJKtc-VF-2.004.woff2` is 13,483,652 bytes;
its SHA256 is
`f334abf0564f39777485bfb4bd7413af6f40eddb6ae3bb3035121d62e00d21b7`.
It was losslessly compressed with FontTools 4.68.0 and Brotli 1.2.1, preserving
the upstream timestamps. CFF2 was selected after native WebKitGTK rejected the
larger full TrueType WOFF2; the CFF2 WOFF2 loads in both WebKitGTK and Chromium.
To reproduce from the verified OTF:

```sh
python3 -m fontTools.ttLib.woff2 compress -o NotoSansCJKtc-VF-2.004.woff2 NotoSansCJKtc-VF.otf
```

Application builds use the committed WOFF2 and license; no font download or
conversion tools are needed to build DLDR.

## Linux runtime

The Linux AppImage additionally contains third-party software, including GTK,
WebKitGTK, GStreamer, libmpv and codec libraries. These components retain their
individual licenses; **the complete AppImage is not exclusively MIT-licensed**.
The selected Ubuntu libmpv/FFmpeg builds include GPL-licensed components.

License notices are included in each application package. Corresponding source
archives are retained on the [Dependency sources](https://github.com/ribeye0/dldr/releases/tag/dependency-sources)
archive page, linked from every release. Inside an extracted AppImage, see `usr/share/doc/dldr/` for:

- `system-packages.json`: actual bundled binary and source package versions.
- `bundled-files.json`: packaged paths and their distribution package owners.
- `system/` and `common-licenses/`: package copyright/license texts.
- `rust-packages.json`, `javascript-packages.json`, `rust/`, `javascript/`:
  dependency information and available license/notice files.

The accompanying runtime source archive includes the exact distribution `.dsc`,
original source and Debian/Ubuntu packaging files, plus vendored Rust sources.
It also includes the pinned outer AppImage runtime source, its FUSE/squashfuse
sources, patches and upstream rebuild instructions; static runtime component
notices are under `bundler-licenses/`.
The version tag in the DLDR repository contains the application and its build/repack scripts.
Use the same-version sources and follow `docs/releasing.md` to rebuild.
No upstream update feed or private signing key is needed for this manual-update
release channel. SHA256 hashes identify the downloaded files; they do not replace
a publisher signature.

The DLDR logo was supplied by the project owner. Genre data comes from DLsite.
Product artwork and purchased media are not distributed in DLDR release packages.

## Windows runtime

Windows bundles pinned MSYS2 UCRT64 builds of libmpv, FFmpeg, libarchive and
only their required DLL dependencies. Exact package versions, selected files,
SHA256 hashes and source-package URLs are recorded in
`tools/release/windows-runtime.json` and shipped under `third-party/`.
The DLLs remain replaceable beside the executable.

These builds include GPL-licensed components; **the complete Windows package is
not exclusively MIT-licensed**. License notices accompany the binaries. The
matching Windows runtime source archive contains each exact MSYS2 source package
(upstream sources, patches and PKGBUILD recipes) and vendored Rust dependencies,
including native C/C++ sources. The DLDR version tag contains the application and its packaging scripts.
Corresponding runtime sources remain available on the separate dependency-source
archive page; each application release links to it.
User credentials and purchased media are never included.
