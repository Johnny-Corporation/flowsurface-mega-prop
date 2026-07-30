#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
WORKTREE_ID="$(basename "$ROOT_DIR" | tr '[:upper:] /:' '[:lower:]---')"
SESSION_NAME="flowsurface-${WORKTREE_ID}-app"
LOG_DIR="$ROOT_DIR/logs"
LOG_FILE="$LOG_DIR/codex-cargo-run.log"

mkdir -p "$LOG_DIR"
cd "$ROOT_DIR"

if ! command -v tmux >/dev/null 2>&1; then
  echo "tmux is required for the run action."
  exit 1
fi

cargo build --workspace

if tmux has-session -t "$SESSION_NAME" 2>/dev/null; then
  tmux kill-session -t "$SESSION_NAME"
fi

tmux new-session -d -s "$SESSION_NAME" -c "$ROOT_DIR" "cargo run 2>&1 | tee '$LOG_FILE'"

echo "Started app from: $ROOT_DIR"
echo "tmux session: $SESSION_NAME"
echo "logs: $LOG_FILE"
