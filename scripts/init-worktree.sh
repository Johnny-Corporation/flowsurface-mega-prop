#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROJECT_NAME="$(basename "$ROOT_DIR")"
DEFAULT_BRANCH_PREFIX="johnny"
DEFAULT_BRANCH_NAME="${DEFAULT_BRANCH_PREFIX}/$(date +%Y%m%d-%H%M)-worktree"

cd "$ROOT_DIR"

if ! git rev-parse --is-inside-work-tree >/dev/null 2>&1; then
  echo "This script must be run from inside the ${PROJECT_NAME} git repository."
  exit 1
fi

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "The current worktree has uncommitted tracked changes."
  echo "The new worktree will still be created from committed HEAD."
  printf "Continue anyway? [y/N]: "
  read -r CONTINUE_WITH_DIRTY_TREE

  if [ "$CONTINUE_WITH_DIRTY_TREE" != "y" ] && [ "$CONTINUE_WITH_DIRTY_TREE" != "Y" ]; then
    echo "Stopped without creating a worktree."
    exit 1
  fi
fi

if git branch --list "matvey/*" | grep -q . && ! git branch --list "johnny/*" | grep -q .; then
  DEFAULT_BRANCH_PREFIX="matvey"
  DEFAULT_BRANCH_NAME="${DEFAULT_BRANCH_PREFIX}/$(date +%Y%m%d-%H%M)-worktree"
fi

echo "Create a new worktree for ${PROJECT_NAME}."
printf "Branch name [%s]: " "$DEFAULT_BRANCH_NAME"
read -r BRANCH_NAME
BRANCH_NAME="${BRANCH_NAME:-$DEFAULT_BRANCH_NAME}"

BRANCH_SLUG="$(printf '%s' "$BRANCH_NAME" | tr '/[:space:]' '--' | tr -cd 'A-Za-z0-9._-')"
DEFAULT_WORKTREE_DIR="$(dirname "$ROOT_DIR")/${PROJECT_NAME}-${BRANCH_SLUG}"

printf "Worktree path [%s]: " "$DEFAULT_WORKTREE_DIR"
read -r WORKTREE_DIR
WORKTREE_DIR="${WORKTREE_DIR:-$DEFAULT_WORKTREE_DIR}"

if [ -e "$WORKTREE_DIR" ]; then
  echo "Refusing to overwrite existing path: $WORKTREE_DIR"
  exit 1
fi

echo
echo "Creating worktree:"
echo "  branch: $BRANCH_NAME"
echo "  path:   $WORKTREE_DIR"
echo

git fetch --all --prune

if git show-ref --verify --quiet "refs/heads/$BRANCH_NAME"; then
  git worktree add "$WORKTREE_DIR" "$BRANCH_NAME"
else
  git worktree add -b "$BRANCH_NAME" "$WORKTREE_DIR"
fi

copy_if_present() {
  local source_path="$1"
  local target_path="$2"

  if [ -e "$source_path" ]; then
    mkdir -p "$(dirname "$target_path")"
    cp -R "$source_path" "$target_path"
    echo "Copied $(basename "$source_path")"
  fi
}

copy_if_present "$ROOT_DIR/AGENTS.md" "$WORKTREE_DIR/AGENTS.md"
copy_if_present "$ROOT_DIR/.vscode" "$WORKTREE_DIR/.vscode"

if [ -f "$ROOT_DIR/.env.example" ]; then
  copy_if_present "$ROOT_DIR/.env.example" "$WORKTREE_DIR/.env.example"
fi

cd "$WORKTREE_DIR"

echo
echo "Preparing Rust toolchain and dependencies."
rustup show active-toolchain >/dev/null
rustup component add clippy rustfmt
cargo fetch

printf "Run cargo check now? [y/N]: "
read -r RUN_CARGO_CHECK

if [ "$RUN_CARGO_CHECK" = "y" ] || [ "$RUN_CARGO_CHECK" = "Y" ]; then
  cargo check --workspace
fi

echo
echo "Worktree is ready:"
echo "  cd \"$WORKTREE_DIR\""
echo
echo "Sensitive files intentionally not copied: .env, logs, target, key material."
echo "If API credentials are needed, create a local ignored file from an example placeholder."
