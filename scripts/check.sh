#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

pnpm --filter @cli-master/desktop lint
pnpm --filter @cli-master/desktop typecheck
pnpm --filter @cli-master/desktop test
pnpm --filter @cli-master/desktop build

cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo build -p cli-master-fake-agent --locked
cargo test --workspace --locked
