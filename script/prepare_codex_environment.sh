#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
cd "$ROOT_DIR"

echo "Preparing Flowsurface Codex environment in: $ROOT_DIR"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo was not found."
  echo "Install Rust/Cargo with rustup, then run this script again:"
  echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
  exit 1
fi

if command -v rustup >/dev/null 2>&1; then
  echo "Installing repository Rust toolchain from rust-toolchain.toml..."
  rustup show >/dev/null
  rustup component add clippy rustfmt
else
  echo "rustup was not found; using existing cargo toolchain."
fi

echo "Fetching Cargo dependencies..."
cargo fetch --locked

echo "Environment is ready."
