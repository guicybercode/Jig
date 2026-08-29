#!/usr/bin/env bash
# Copy cli-masterd into src-tauri/binaries with a target-triple suffix so Tauri
# can embed it as an externalBin sidecar next to the desktop executable.
set -euo pipefail

profile="${1:-release}"
root="$(git -C "$(dirname "$0")" rev-parse --show-toplevel)"
cd "$root"

case "$profile" in
  release)
    cargo build -p cli-master-daemon --release
    src="$root/target/release/cli-masterd"
    ;;
  debug)
    cargo build -p cli-master-daemon
    src="$root/target/debug/cli-masterd"
    ;;
  *)
    echo "usage: $0 [debug|release]" >&2
    exit 2
    ;;
esac

if [[ ! -x "$src" ]]; then
  echo "expected daemon binary at $src" >&2
  exit 1
fi

triple="$(rustc -vV | awk '/^host:/{print $2}')"
if [[ -z "$triple" ]]; then
  echo "could not read rustc host triple" >&2
  exit 1
fi

dest_dir="$root/apps/desktop/src-tauri/binaries"
mkdir -p "$dest_dir"
dest="$dest_dir/cli-masterd-$triple"
cp "$src" "$dest"
chmod +x "$dest"
echo "staged $dest"
