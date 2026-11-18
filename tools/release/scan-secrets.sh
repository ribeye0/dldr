#!/usr/bin/env bash
set -euo pipefail
cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.."
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT
curl --fail --location --silent --show-error -o "$work/gitleaks.tar.gz" \
  https://github.com/gitleaks/gitleaks/releases/download/v8.30.1/gitleaks_8.30.1_linux_x64.tar.gz
echo "551f6fc83ea457d62a0d98237cbad105af8d557003051f41f3e7ca7b3f2470eb  $work/gitleaks.tar.gz" | sha256sum --check --strict -
tar -xzf "$work/gitleaks.tar.gz" -C "$work" gitleaks
mkdir -p target/security
"$work/gitleaks" git --redact --no-banner --report-format json --report-path target/security/history.json
# Include uncommitted/untracked source when run locally; never traverse profiles,
# build output or dependency stores excluded from the source distribution.
python3 - "$work/source" <<'PY'
from pathlib import Path
import shutil,subprocess,sys
destination=Path(sys.argv[1]);destination.mkdir()
for raw in subprocess.check_output(['git','ls-files','--cached','--others','--exclude-standard','-z']).split(b'\0'):
    if not raw: continue
    source=Path(raw.decode())
    if not source.is_file(): continue
    target=destination/source;target.parent.mkdir(parents=True,exist_ok=True)
    shutil.copyfile(source,target)
PY
"$work/gitleaks" dir "$work/source" --redact --no-banner --report-format json --report-path target/security/source.json
