#!/usr/bin/env bash
# Write SHA-256 checksums for every regular file in a directory.
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: checksums.sh <artifact-directory>" >&2
  exit 2
fi

artifact_dir="$1"
if [[ ! -d "$artifact_dir" ]]; then
  echo "not a directory: $artifact_dir" >&2
  exit 1
fi

cd "$artifact_dir"

checksum() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum -- "$file"
  else
    shasum -a 256 -- "$file"
  fi
}

tmp="$(mktemp)"
count=0
while IFS= read -r file; do
  [[ -z "$file" ]] && continue
  checksum "$file" >> "$tmp"
  count=$((count + 1))
done <<EOF
$(find . -maxdepth 1 -type f ! -name SHA256SUMS | sed 's|^\./||' | LC_ALL=C sort)
EOF

if [[ "$count" -eq 0 ]]; then
  rm -f "$tmp"
  echo "no artifact files in $artifact_dir" >&2
  exit 1
fi

mv "$tmp" SHA256SUMS
echo "wrote $artifact_dir/SHA256SUMS"
cat SHA256SUMS
