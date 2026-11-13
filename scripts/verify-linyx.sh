#!/usr/bin/env bash
set -euo pipefail

CRATE="${1:-atlas-core}"
MODE="${2:-check}"   # check | test | clippy

echo "==> Verifying $CRATE ($MODE)"

case "$MODE" in
  check)  cargo check  -p "$CRATE" --all-features ;;
  test)   RUST_MIN_STACK=16777216 cargo test -p "$CRATE" --all-features --no-fail-fast ;;
  clippy) cargo clippy -p "$CRATE" --all-features --all-targets -- -D warnings ;;
  *) echo "unknown mode: $MODE" >&2; exit 2 ;;
esac

echo "==> OK: $CRATE ($MODE)"
