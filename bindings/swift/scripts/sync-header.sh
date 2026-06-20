#!/usr/bin/env bash
# Sync the vendored C header in bindings/swift/Sources/CRemem/include/rememhq.h
# from the canonical source at rememhq-core/include/rememhq.h.
#
# Run this from anywhere; it locates the repo root from its own path.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

SRC="$REPO_ROOT/rememhq-core/include/rememhq.h"
DST="$REPO_ROOT/bindings/swift/Sources/CRemem/include/rememhq.h"

if [[ ! -f "$SRC" ]]; then
  echo "error: canonical header not found at $SRC" >&2
  exit 1
fi

# Preserve the "vendored copy" provenance banner instead of overwriting it,
# by re-inserting it after the first comment block of the canonical file.
{
  echo "// rememhq.h — C ABI for rememhq-core's high-level reasoning engine."
  echo "//"
  echo "// ⚠️ VENDORED COPY. The source of truth is"
  echo "// \`rememhq-core/include/rememhq.h\` in the main remem repository. This copy"
  echo "// exists so the Swift package can build standalone (e.g. via Swift Package"
  echo "// Index) without checking out the whole monorepo. Run"
  echo "// \`bindings/swift/scripts/sync-header.sh\` from the repo root after editing"
  echo "// the canonical header to keep this copy in sync."
  tail -n +3 "$SRC"
} > "$DST"

echo "Synced $DST from $SRC"
