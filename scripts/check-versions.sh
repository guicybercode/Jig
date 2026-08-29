#!/usr/bin/env bash
# Fail if Cargo, npm, Tauri, and the protocol catalog disagree on versions.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"

python3 - "$root" <<'PY'
import json
import re
import sys
from pathlib import Path

root = Path(sys.argv[1])

def cargo_workspace_version(text: str) -> str:
    match = re.search(
        r"\[workspace\.package\][^\[]*?^version\s*=\s*\"([^\"]+)\"",
        text,
        flags=re.MULTILINE | re.DOTALL,
    )
    if not match:
        raise SystemExit("could not read [workspace.package].version from Cargo.toml")
    return match.group(1)

cargo = cargo_workspace_version((root / "Cargo.toml").read_text())
root_pkg = json.loads((root / "package.json").read_text())["version"]
desktop_pkg = json.loads((root / "apps/desktop/package.json").read_text())["version"]
tauri = json.loads((root / "apps/desktop/src-tauri/tauri.conf.json").read_text())["version"]
catalog = json.loads((root / "protocol/catalog.json").read_text())
catalog_app = catalog["applicationVersion"]
catalog_protocol = catalog["protocolVersion"]

versions = {
    "Cargo.toml [workspace.package]": cargo,
    "package.json": root_pkg,
    "apps/desktop/package.json": desktop_pkg,
    "apps/desktop/src-tauri/tauri.conf.json": tauri,
    "protocol/catalog.json applicationVersion": catalog_app,
}

print("application versions:")
for name, value in versions.items():
    print(f"  {name}: {value}")
print(f"protocol version: {catalog_protocol}")

mismatched = sorted({value for value in versions.values()})
if len(mismatched) != 1:
    raise SystemExit("application versions do not match")
if catalog_protocol != 1:
    raise SystemExit(f"protocol/catalog.json protocolVersion must be 1, got {catalog_protocol}")
print("version check passed")
PY
