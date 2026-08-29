#!/usr/bin/env bash
# Stage cli-masterd, build platform bundles, smoke-test them, and write checksums.
set -euo pipefail

root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root"

bash "$root/scripts/check-versions.sh"
bash "$root/scripts/stage-sidecar.sh"
pnpm --filter @cli-master/desktop tauri build
bash "$root/scripts/smoke-bundle.sh"
artifact_dir="$root/dist/artifacts"
rm -rf "$artifact_dir"
mkdir -p "$artifact_dir"

case "$(uname -s)" in
  Linux)
    find "$root/target/release/bundle" -type f \( -name '*.AppImage' -o -name '*.deb' \) -exec cp {} "$artifact_dir/" \;
    ;;
  Darwin)
    find "$root/target/release/bundle" -type f -name '*.dmg' -exec cp {} "$artifact_dir/" \;
    ;;
  *)
    echo "unsupported packaging host: $(uname -s)" >&2
    exit 1
    ;;
esac

bash "$root/scripts/checksums.sh" "$artifact_dir"
