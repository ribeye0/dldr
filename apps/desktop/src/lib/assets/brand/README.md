# DLDR brand assets

DLDR is short for **DLsite Downloader**.

- `dldr-source.png`: original artwork.
- `dldr-logo.png`: cropped transparent logo used in the sidebar.
- `dldr-icon.png`: square symbol used to generate desktop icons.

To regenerate the transparent assets, install Pillow and NumPy, then run:

```sh
python3 tools/dev/prepare-brand.py
pnpm tauri icon src/lib/assets/brand/dldr-icon.png -o /tmp/dldr-native-icons
```

Copy the matching desktop icon sizes to `apps/desktop/src-tauri/icons/` and `64x64.png` to
`apps/desktop/static/favicon.png`. The extraction script preserves the original pixels and
banner lettering, removes the navy background and writes temporary previews to
`/tmp/`. Asset generation is independent of the application build.
