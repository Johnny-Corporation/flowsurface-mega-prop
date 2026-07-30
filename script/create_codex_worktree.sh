#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"
PROJECT_NAME="$(basename "$ROOT_DIR")"
DEFAULT_SLUG="codex-$(date +%Y%m%d-%H%M%S)"
DEFAULT_BRANCH="johnny/feat/$DEFAULT_SLUG"
DEFAULT_WORKTREE_ROOT="$HOME/.codex/worktrees/manual"

cd "$ROOT_DIR"

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "This script must be run from a git repository."
  exit 1
fi

printf "Branch name [%s]: " "$DEFAULT_BRANCH"
read -r BRANCH_NAME
BRANCH_NAME="${BRANCH_NAME:-$DEFAULT_BRANCH}"

SAFE_BRANCH_NAME="$(printf '%s' "$BRANCH_NAME" | tr '/: ' '---')"
DEFAULT_WORKTREE_PATH="$DEFAULT_WORKTREE_ROOT/$SAFE_BRANCH_NAME/$PROJECT_NAME"

printf "Worktree path [%s]: " "$DEFAULT_WORKTREE_PATH"
read -r WORKTREE_PATH
WORKTREE_PATH="${WORKTREE_PATH:-$DEFAULT_WORKTREE_PATH}"

if git show-ref --verify --quiet "refs/heads/$BRANCH_NAME"; then
  echo "Branch already exists: $BRANCH_NAME"
  echo "Creating worktree from the existing branch."
  git worktree add "$WORKTREE_PATH" "$BRANCH_NAME"
else
  echo "Creating branch and worktree: $BRANCH_NAME"
  git worktree add -b "$BRANCH_NAME" "$WORKTREE_PATH"
fi

if [ -f "$ROOT_DIR/AGENTS.md" ]; then
  cp "$ROOT_DIR/AGENTS.md" "$WORKTREE_PATH/AGENTS.md"
  echo "Copied ignored AGENTS.md into the new worktree."
else
  echo "No local AGENTS.md found to copy."
fi

if [ -d "$ROOT_DIR/.codex" ]; then
  mkdir -p "$WORKTREE_PATH/.codex"
  cp -R "$ROOT_DIR/.codex/." "$WORKTREE_PATH/.codex/"
  echo "Copied local .codex environment files into the new worktree."
fi

"$WORKTREE_PATH/script/prepare_codex_environment.sh"

echo
echo "Created Codex-ready worktree:"
echo "  path: $WORKTREE_PATH"
echo "  branch: $BRANCH_NAME"
echo
echo "Run action:"
echo "  cd '$WORKTREE_PATH' && script/build_and_run.sh"
