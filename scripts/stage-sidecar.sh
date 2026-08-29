#!/usr/bin/env bash
# Copy cli-masterd into apps/desktop/src-tauri/binaries with a target-triple suffix.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
profile="release"
if [[ "${1:-}" == "--debug" ]]; then
  profile="dev"
fi

triple="$(rustc -vV | awk '/^host:/{print $2}')"
if [[ -z "$triple" ]]; then
  echo "could not read rustc host triple" >&2
  exit 1
fi

if [[ "$profile" == "dev" ]]; then
  cargo build -p cli-master-daemon --locked --manifest-path "$root/Cargo.toml"
  source_bin="$root/target/debug/cli-masterd"
else
  cargo build -p cli-master-daemon --release --locked --manifest-path "$root/Cargo.toml"
  source_bin="$root/target/release/cli-masterd"
fi

if [[ ! -f "$source_bin" ]]; then
  echo "missing daemon binary: $source_bin" >&2
  exit 1
fi

dest_dir="$root/apps/desktop/src-tauri/binaries"
mkdir -p "$dest_dir"
dest="$dest_dir/cli-masterd-${triple}"
cp "$source_bin" "$dest"
chmod 755 "$dest"
echo "staged $dest"
