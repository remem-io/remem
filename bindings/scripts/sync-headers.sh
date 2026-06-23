#!/usr/bin/env bash
# Sync every vendored copy of rememhq.h across all bindings from the
# canonical source at rememhq-core/include/rememhq.h.
#
# Run this from anywhere; it locates the repo root from its own path.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

SRC="$REPO_ROOT/rememhq-core/include/rememhq.h"

if [[ ! -f "$SRC" ]]; then
  echo "error: canonical header not found at $SRC" >&2
  exit 1
fi

# Each vendored copy gets the canonical content with its own "vendored
# copy" provenance banner re-inserted after the first comment line,
# rather than overwriting whatever banner already explains why that
# particular copy exists. $reason is wrapped to roughly match the rest
# of the header's ~78-column comment width, rather than dumped as one
# long unwrapped line.
sync_one() {
  local dst="$1"
  local reason="$2"

  mkdir -p "$(dirname "$dst")"
  {
    echo "// rememhq.h — C ABI for rememhq-core's high-level reasoning engine."
    echo "//"
    echo "// ⚠️ VENDORED COPY. The source of truth is"
    echo "// \`rememhq-core/include/rememhq.h\` in the main remem repository."
    echo "$reason" | fold -s -w 75 | sed -e 's/^/\/\/ /' -e 's/[[:space:]]*$//'
    echo "// Run \`bindings/scripts/sync-headers.sh\` from the repo root after"
    echo "// editing the canonical header to keep this copy in sync."
    tail -n +3 "$SRC"
  } > "$dst"

  echo "Synced $dst"
}

sync_one \
  "$REPO_ROOT/bindings/swift/Sources/CRemem/include/rememhq.h" \
  "This copy exists so the Swift package can build standalone (e.g. via Swift Package Index) without checking out the whole monorepo."

sync_one \
  "$REPO_ROOT/bindings/react-native/ios/RememCore/rememhq.h" \
  "This copy exists so the Expo module can build standalone (e.g. when consumed as an npm package) without checking out the whole monorepo."

echo "Done."
