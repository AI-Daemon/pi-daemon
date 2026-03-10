#!/usr/bin/env bash
set -euo pipefail

echo "=============================================="
echo "     pi-daemon local test suite"
echo "=============================================="
echo ""

FAILED=0

# Phase 1: Lint
echo "▸ [1/5] Formatting check..."
if ! cargo fmt --all -- --check 2>&1; then
    echo "❌ Formatting failed. Run: cargo fmt --all"
    FAILED=1
else
    echo "  ✅ Formatting OK"
fi

echo ""
echo "▸ [2/5] Clippy..."
if ! cargo clippy --all-targets --all-features -- -D warnings 2>&1; then
    echo "❌ Clippy warnings found"
    FAILED=1
else
    echo "  ✅ Clippy OK"
fi

echo ""
echo "▸ [3/5] Doc check..."
if ! RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features 2>&1; then
    echo "❌ Doc warnings found"
    FAILED=1
else
    echo "  ✅ Docs OK"
fi

# Phase 2: Tests
echo ""
echo "▸ [4/5] Unit tests (all crates)..."
if ! cargo test --all 2>&1; then
    echo "❌ Unit tests failed"
    FAILED=1
else
    echo "  ✅ Unit tests OK"
fi

# Phase 3: Integration tests
echo ""
echo "▸ [5/5] Integration tests..."
if ! cargo test --all --test '*' 2>&1; then
    echo "❌ Integration tests failed"
    FAILED=1
else
    echo "  ✅ Integration tests OK"
fi

echo ""
echo "=============================================="
if [ "$FAILED" -ne 0 ]; then
    echo "❌ Local tests FAILED — fix issues before pushing"
    exit 1
else
    echo "✅ All local tests passed — safe to push"
fi
echo "=============================================="
