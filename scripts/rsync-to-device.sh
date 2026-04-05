#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"

DRY_RUN=false
DELETE=false
FORCE_DELETE=false
PRUNE_EXCLUDED=false
INCLUDE_GIT=false
INCLUDE_DEPS=false

usage() {
  cat <<'USAGE'
Usage: scripts/rsync-to-device.sh [options] <target>

Sync this repository to a target device/path using rsync.

Examples:
  scripts/rsync-to-device.sh dietpi@192.168.1.42:/home/dietpi/supafast-jellyfin
  scripts/rsync-to-device.sh --dry-run --delete user@host:/srv/supafast-jellyfin
  scripts/rsync-to-device.sh /mnt/usb/supafast-jellyfin

Options:
  -n, --dry-run         Show what would be synced without writing changes
  -d, --delete          Delete files on target that no longer exist in source
      --force-delete    Required with --delete to confirm destructive sync intent
      --prune-excluded  Also delete excluded paths on target (very destructive)
      --include-git     Include .git directory in sync
      --include-deps    Include heavy dependency/build folders (target, node_modules)
  -h, --help            Show this help
USAGE
}

validate_root_dir() {
  if [[ -z "$ROOT_DIR" || "$ROOT_DIR" == "/" ]]; then
    echo "Invalid repository root: '$ROOT_DIR'" >&2
    exit 1
  fi

  if [[ ! -f "$ROOT_DIR/Cargo.toml" || ! -f "$ROOT_DIR/frontend/package.json" ]]; then
    echo "Refusing to sync: '$ROOT_DIR' does not look like this repository" >&2
    exit 1
  fi
}

require_rsync() {
  if ! command -v rsync >/dev/null 2>&1; then
    echo "rsync is required but was not found in PATH" >&2
    exit 1
  fi
}

extract_target_path() {
  local raw_target="$1"
  if [[ "$raw_target" == *:* && "$raw_target" != /*:* ]]; then
    echo "${raw_target#*:}"
    return 0
  fi

  echo "$raw_target"
}

normalize_target_path() {
  local value="$1"

  while [[ "$value" == *"//"* ]]; do
    value="${value//\/\//\/}"
  done

  while [[ "$value" == */ && "$value" != "/" ]]; do
    value="${value%/}"
  done

  echo "$value"
}

validate_delete_target() {
  local raw_target="$1"
  local target_path normalized_target

  target_path="$(extract_target_path "$raw_target")"
  normalized_target="$(normalize_target_path "$target_path")"

  if [[ -z "$normalized_target" || "$normalized_target" == "/" ]]; then
    echo "Refusing to run --delete against risky target path: '$raw_target'" >&2
    echo "Provide a specific project directory target, not a filesystem root." >&2
    exit 1
  fi

  if [[ "$normalized_target" != /* ]]; then
    echo "Refusing to run --delete against non-absolute target path: '$raw_target'" >&2
    echo "Use an absolute project directory path for delete mode." >&2
    exit 1
  fi

  case "$normalized_target" in
    /home|/root|/Users|/var|/srv|/opt|/etc|/usr|/mnt|/media)
      echo "Refusing to run --delete against high-level target path: '$raw_target'" >&2
      echo "Provide a deeper directory path (for example .../supafast-jellyfin)." >&2
      exit 1
      ;;
  esac
}

TARGET=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    -n|--dry-run)
      DRY_RUN=true
      ;;
    -d|--delete)
      DELETE=true
      ;;
    --force-delete)
      FORCE_DELETE=true
      ;;
    --prune-excluded)
      PRUNE_EXCLUDED=true
      ;;
    --include-git)
      INCLUDE_GIT=true
      ;;
    --include-deps)
      INCLUDE_DEPS=true
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
    *)
      if [[ -n "$TARGET" ]]; then
        echo "Only one target is allowed; got extra argument: $1" >&2
        usage >&2
        exit 1
      fi
      TARGET="$1"
      ;;
  esac
  shift
done

if [[ -z "$TARGET" && $# -gt 0 ]]; then
  TARGET="$1"
  shift
fi

if [[ -z "$TARGET" ]]; then
  echo "Missing target argument." >&2
  usage >&2
  exit 1
fi

if [[ $# -gt 0 ]]; then
  echo "Unexpected trailing arguments: $*" >&2
  usage >&2
  exit 1
fi

validate_root_dir
require_rsync

if [[ "$DELETE" == true && "$FORCE_DELETE" == false ]]; then
  echo "Refusing destructive sync: --delete requires --force-delete" >&2
  exit 1
fi

if [[ "$DELETE" == true ]]; then
  validate_delete_target "$TARGET"
fi

RSYNC_ARGS=(
  --archive
  --compress
  --human-readable
  --itemize-changes
  --prune-empty-dirs
  --exclude=.env
  --exclude=.env.*
  --exclude=.DS_Store
  --exclude=Thumbs.db
  --exclude=.claude/
  --exclude=tmp/
)

if [[ "$INCLUDE_GIT" == false ]]; then
  RSYNC_ARGS+=(--exclude=.git/)
fi

if [[ "$INCLUDE_DEPS" == false ]]; then
  RSYNC_ARGS+=(
    --exclude=target/
    --exclude=frontend/node_modules/
    --exclude=frontend/.svelte-kit/
    --exclude=frontend/.vite/
    --exclude=frontend/build/
    --exclude=frontend/dist/
  )
fi

if [[ "$DELETE" == true ]]; then
  RSYNC_ARGS+=(--delete)
fi

if [[ "$PRUNE_EXCLUDED" == true ]]; then
  if [[ "$DELETE" == false ]]; then
    echo "--prune-excluded requires --delete" >&2
    exit 1
  fi
  RSYNC_ARGS+=(--delete-excluded)
fi

if [[ "$DRY_RUN" == true ]]; then
  RSYNC_ARGS+=(--dry-run)
  echo "Dry run enabled: no changes will be written"
fi

echo "Sync source: $ROOT_DIR/"
echo "Sync target: $TARGET"

echo "Running rsync..."
rsync "${RSYNC_ARGS[@]}" "$ROOT_DIR/" "$TARGET"

echo "Sync complete."
