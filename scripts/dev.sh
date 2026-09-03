#!/usr/bin/env bash
# Development loop: vite + tauri app with hot reload, plus the extension in watch mode.
#   scripts/dev.sh            # normal dev (real data dir, Touch ID prompts)
#   scripts/dev.sh --sandbox  # PASSVALET_HOME=/tmp/passvalet-dev, presence checks skipped
set -euo pipefail
cd "$(dirname "$0")/.."

if [[ "${1:-}" == "--sandbox" ]]; then
  export PASSVALET_HOME=/tmp/passvalet-dev
  export PASSVALET_DEV_SKIP_PRESENCE=1
  mkdir -p "$PASSVALET_HOME"
  echo "sandbox mode: data in $PASSVALET_HOME, Touch ID skipped (debug builds only)"
fi

cargo build -p passvalet-cli
mkdir -p apps/desktop/src-tauri/binaries
cp target/debug/passvalet "apps/desktop/src-tauri/binaries/passvalet-$(rustc -vV | sed -n 's/^host: //p')"

(pnpm --filter @passvalet/extension dev &) 
exec pnpm --filter @passvalet/desktop tauri dev
