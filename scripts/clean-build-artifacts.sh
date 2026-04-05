#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

DRY_RUN=false
INCLUDE_DEPS=false

usage() {
  cat <<'USAGE'
Usage: scripts/clean-build-artifacts.sh [options]

Removes generated build artifacts for the Rust backend and Svelte frontend.
Requires: bash

Options:
  -n, --dry-run       Show what would be removed without deleting anything
  -a, --all           Also remove dependency caches (frontend/node_modules)
  -h, --help          Show this help
USAGE
}

validate_root_dir() {
  if [[ -z "$ROOT_DIR" || "$ROOT_DIR" == "/" ]]; then
    echo "Refusing to clean: invalid repository root '$ROOT_DIR'" >&2
    exit 1
  fi

  if [[ ! -f "$ROOT_DIR/Cargo.toml" || ! -f "$ROOT_DIR/frontend/package.json" ]]; then
    echo "Refusing to clean: '$ROOT_DIR' does not look like this repository" >&2
    exit 1
  fi
}

ensure_inside_root() {
  local path="$1"
  case "$path" in
    "$ROOT_DIR"/*) return 0 ;;
    *)
      echo "Refusing to remove path outside repository: $path" >&2
      exit 1
      ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    -n|--dry-run)
      DRY_RUN=true
      ;;
    -a|--all)
      INCLUDE_DEPS=true
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
  shift
done

TARGETS=(
  "target"
  "frontend/.svelte-kit"
  "frontend/dist"
  "frontend/build"
  "frontend/node_modules/.vite"
  "frontend/.vite"
  "tmp/logs"
)

if [[ "$INCLUDE_DEPS" == true ]]; then
  TARGETS+=("frontend/node_modules")
fi

remove_target() {
  local rel_path="$1"
  local abs_path="$ROOT_DIR/$rel_path"

  if [[ ! -e "$abs_path" ]]; then
    return 0
  fi

  ensure_inside_root "$abs_path"

  if [[ "$DRY_RUN" == true ]]; then
    echo "Would remove: $rel_path"
    return 0
  fi

  rm -rf "$abs_path"
  echo "Removed: $rel_path"
}

if [[ "$DRY_RUN" == true ]]; then
  echo "Dry run: no files will be removed"
fi

validate_root_dir

for target in "${TARGETS[@]}"; do
  remove_target "$target"
done

echo "Build artifact cleanup complete."
