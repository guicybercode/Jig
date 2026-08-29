#!/usr/bin/env bash
# Inspect a Tauri bundle for cli-masterd and run --version plus --preflight.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
bundle_root="${1:-$root/target/release/bundle}"

if [[ ! -d "$bundle_root" ]]; then
  echo "bundle directory not found: $bundle_root" >&2
  echo "Build installable artifacts with: pnpm tauri:build" >&2
  exit 2
fi

find_first() {
  local pattern="$1"
  find "$bundle_root" -type f -name "$pattern" -print | sort | head -n 1
}

daemon=""
work="$(mktemp -d "${TMPDIR:-/tmp}/cli-master-smoke.XXXXXX")"
cleanup() {
  rm -rf "$work"
}
trap cleanup EXIT

case "$(uname -s)" in
  Linux)
    appimage="$(find_first '*.AppImage')"
    if [[ -z "$appimage" ]]; then
      echo "no AppImage under $bundle_root" >&2
      find "$bundle_root" -type f | sort >&2 || true
      exit 1
    fi
    echo "smoke AppImage: $appimage"
    chmod +x "$appimage"
    (
      cd "$work"
      "$appimage" --appimage-extract >/dev/null
    )
    daemon="$(find "$work/squashfs-root" -type f -name 'cli-masterd' -print | sort | head -n 1)"
    desktop="$(find "$work/squashfs-root" -type f \( -name 'cli-master-desktop' -o -name 'CLI Master' \) -print | sort | head -n 1)"
    if [[ -z "$desktop" ]]; then
      echo "desktop binary missing from AppImage" >&2
      exit 1
    fi
    echo "desktop binary: $desktop"
    ;;
  Darwin)
    app="$(find "$bundle_root" -type d -name '*.app' -print | sort | head -n 1)"
    if [[ -z "$app" ]]; then
      echo "no .app bundle under $bundle_root" >&2
      find "$bundle_root" -print | sort >&2 || true
      exit 1
    fi
    echo "smoke app: $app"
    daemon="$app/Contents/MacOS/cli-masterd"
    desktop="$(find "$app/Contents/MacOS" -maxdepth 1 -type f ! -name 'cli-masterd' -print | sort | head -n 1)"
    if [[ -z "$desktop" ]]; then
      echo "desktop binary missing from $app" >&2
      exit 1
    fi
    dmg="$(find_first '*.dmg')"
    if [[ -n "$dmg" ]]; then
      echo "dmg present: $dmg"
    else
      echo "warning: no DMG was produced" >&2
    fi
    ;;
  *)
    echo "unsupported smoke host: $(uname -s)" >&2
    exit 1
    ;;
esac

if [[ -z "$daemon" || ! -f "$daemon" ]]; then
  echo "cli-masterd missing from bundle" >&2
  exit 1
fi
echo "daemon binary: $daemon"

version_out="$("$daemon" --version)"
echo "$version_out"
if [[ "$version_out" != *'protocol 1'* ]]; then
  echo "daemon --version did not report protocol 1" >&2
  exit 1
fi

home="$work/home"
mkdir -p "$home/share" "$home/config" "$home/cache" "$home/state" "$home/run"
preflight_json="$work/preflight.json"

if ! env HOME="$home" \
  XDG_DATA_HOME="$home/share" \
  XDG_CONFIG_HOME="$home/config" \
  XDG_CACHE_HOME="$home/cache" \
  XDG_STATE_HOME="$home/state" \
  XDG_RUNTIME_DIR="$home/run" \
  "$daemon" --preflight >"$preflight_json"; then
  echo "bundled cli-masterd --preflight failed" >&2
  cat "$preflight_json" >&2 || true
  exit 1
fi

python3 - "$preflight_json" <<'PY'
import json
import sys
from pathlib import Path

report = json.loads(Path(sys.argv[1]).read_text())
if not report.get("ok"):
    raise SystemExit(f"preflight ok is false: {report}")
if not report.get("git", {}).get("available"):
    raise SystemExit("bundled daemon did not find Git")
if report.get("protocolVersion") != 1:
    raise SystemExit("protocolVersion must be 1")
print("preflight ok")
PY

echo "bundle smoke passed"
