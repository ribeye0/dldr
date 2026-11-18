#!/usr/bin/env python3
"""Extract the supplied DLDR logo without redrawing its artwork.

Requires Pillow and NumPy. Run from any directory; outputs are repo-relative.
The navy lettering enclosed by the bottom banner remains part of the artwork.
"""
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFilter, ImageOps

ROOT = Path(__file__).resolve().parents[2]
BRAND = ROOT / 'apps/desktop/src/lib/assets/brand'
source = Image.open(BRAND / 'dldr-source.png').convert('RGB')
rgb = np.asarray(source, dtype=np.float64)
red = rgb[:, :, 0]
# Navy background has R <= 3; white and pink artwork have high red values.
# The local foreground red level also recovers the original antialiased edge.
local_red = np.asarray(source.getchannel('R').filter(ImageFilter.MaxFilter(9)), dtype=float)
alpha = np.clip((red - 1) / np.maximum(local_red - 1, 1), 0, 1)
alpha[(red <= 4) | (local_red < 160)] = 0
# Only edge pixels need matting; preserve original interior colors exactly.
near_background = np.asarray(Image.fromarray((red <= 4).astype('uint8') * 255)
                             .filter(ImageFilter.MaxFilter(7))) > 0
alpha[(~near_background) & (red > 180)] = 1
# Fill holes only in the lower banner, preserving its intentional navy lettering.
# Do not fill holes in the folders, arrow, or large DLDR lettering.
banner = Image.fromarray((red[1020:1200] > 128).astype('uint8') * 255)
flood = ImageOps.expand(banner, border=1, fill=0)
ImageDraw.floodfill(flood, (0, 0), 128)
filled_holes = np.asarray(flood)[1:-1, 1:-1] == 0
alpha[1020:1200][filled_holes] = 1
# Unmix the navy contribution at partially transparent edges to avoid a blue halo.
background = np.array([1.0, 23.0, 106.0])
foreground = np.clip((rgb - (1 - alpha[:, :, None]) * background)
                     / np.maximum(alpha[:, :, None], 1 / 255), 0, 255)
foreground[alpha == 0] = 0
rgba = np.dstack((np.rint(foreground), np.rint(alpha * 255))).astype('uint8')
logo = Image.fromarray(rgba)
# A tight transparent master makes the sidebar artwork readable at its actual size.
logo = logo.crop(logo.getbbox())
logo.save(BRAND / 'dldr-logo.png', optimize=True)
# The symbol ends above the large DLDR wordmark. Keep it square for native icons.
symbol = Image.fromarray(rgba).crop((0, 0, source.width, 722))
symbol = symbol.crop(symbol.getbbox())
size = round(max(symbol.size) * 1.16)
icon = Image.new('RGBA', (size, size))
icon.alpha_composite(symbol, ((size - symbol.width) // 2, (size - symbol.height) // 2))
icon.resize((1024, 1024), Image.Resampling.LANCZOS).save(BRAND / 'dldr-icon.png', optimize=True)
print(f'Logo: {logo.size}; icon: 1024×1024; source unchanged')
# Write previews outside the source tree.
for name, color in [('light', '#f5faff'), ('navy', '#092a83')]:
    preview = Image.new('RGBA', logo.size, color)
    preview.alpha_composite(logo)
    preview.convert('RGB').resize((600, round(600 * logo.height / logo.width))).save(f'/tmp/dldr-cutout-{name}.png')
