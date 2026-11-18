#!/usr/bin/env python3
"""Ship CJK fallback glyphs without changing the user's global font settings."""
from pathlib import Path
import shutil
import sys

appdir = Path(sys.argv[1])
font = Path('/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc')
destination = appdir / font.relative_to('/')
destination.parent.mkdir(parents=True, exist_ok=True)
shutil.copyfile(font, destination)
config = appdir / 'usr/share/dldr/fonts.conf'
config.parent.mkdir(parents=True, exist_ok=True)
config.write_text('''<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <include ignore_missing="yes">/etc/fonts/fonts.conf</include>
  <dir prefix="relative">../fonts/opentype/noto</dir>
  <cachedir prefix="xdg">fontconfig</cachedir>
</fontconfig>
''')
launcher = appdir / 'AppRun'
text = launcher.read_text()
line = 'export FONTCONFIG_FILE="$this_dir/usr/share/dldr/fonts.conf"\n'
if line not in text:
    assert 'exec "$this_dir"/AppRun.wrapped' in text
    launcher.write_text(text.replace('exec "$this_dir"/AppRun.wrapped', line + 'exec "$this_dir"/AppRun.wrapped'))
