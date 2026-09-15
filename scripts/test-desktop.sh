#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != Darwin ]]; then
  echo '真实桌面测试需要macOS，请使用GitHub Actions。' >&2
  exit 1
fi

export PASSVALET_HOME="$(mktemp -d /tmp/passvalet-e2e.XXXXXX)"
export PASSVALET_SOCKET="$PASSVALET_HOME/passvalet.sock"
trap 'rm -rf "$PASSVALET_HOME"' EXIT
pnpm exec wdio run ./wdio.conf.mjs
