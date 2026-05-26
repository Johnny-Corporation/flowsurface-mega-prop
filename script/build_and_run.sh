#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SESSION_NAME="flowsurface-megaprop-app"
LOG_DIR="$ROOT_DIR/logs"
LOG_FILE="$LOG_DIR/codex-cargo-run.log"

mkdir -p "$LOG_DIR"
cd "$ROOT_DIR"

cargo build --workspace

if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  tmux kill-session -t "$SESSION_NAME"
fi

tmux new-session -d -s "$SESSION_NAME" -c "$ROOT_DIR" "cargo run 2>&1 | tee '$LOG_FILE'"

echo "Started app from: $ROOT_DIR"
echo "tmux session: $SESSION_NAME"
echo "logs: $LOG_FILE"
