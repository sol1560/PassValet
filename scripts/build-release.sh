#!/usr/bin/env bash
# Build everything for distribution:
#   1. release CLI  -> bundled into the app as the `passvalet` sidecar
#   2. Tauri app    -> apps/desktop/src-tauri/target/release/bundle/{macos,dmg}
#   3. extension    -> apps/extension/.output/passvalet-<ver>-chrome.zip
#
# Signing / notarization (optional, needs an Apple Developer account):
#   export APPLE_SIGNING_IDENTITY="Developer ID Application: Name (TEAMID)"
#   export APPLE_ID=you@example.com APPLE_PASSWORD=app-specific-pw APPLE_TEAM_ID=TEAMID
# Without these, tauri produces an ad-hoc signed .app that runs locally (Gatekeeper will warn
# on other machines). Native passkeys additionally need a provisioning profile with the
# Associated Domains capability — see docs/passkey-setup.md.
set -euo pipefail
cd "$(dirname "$0")/.."

TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
echo "==> building CLI (release, $TRIPLE)"
cargo build --release --locked -p passvalet-cli
mkdir -p apps/desktop/src-tauri/binaries
cp "target/release/passvalet" "apps/desktop/src-tauri/binaries/passvalet-${TRIPLE}"

echo "==> building extension"
pnpm --filter @passvalet/extension build
pnpm --filter @passvalet/extension zip

echo "==> building desktop app"
pnpm --filter @passvalet/desktop tauri build "$@"

shopt -s nullglob
apps=(target/release/bundle/macos/*.app)
dmgs=(target/release/bundle/dmg/*.dmg)
extensions=(apps/extension/.output/*chrome*.zip)
if (( ${#apps[@]} == 0 || ${#dmgs[@]} == 0 || ${#extensions[@]} == 0 )); then
  echo "error: required app, dmg or Chrome extension zip is missing" >&2
  exit 1
fi
for app in "${apps[@]}"; do
  if [[ ! -x "$app/Contents/MacOS/passvalet-desktop" || ! -x "$app/Contents/MacOS/passvalet" ]]; then
    echo "error: desktop executable or bundled CLI is missing from $app" >&2
    exit 1
  fi
done

echo
echo "artifacts:"
printf '%s\n' "${apps[@]}" "${dmgs[@]}" "${extensions[@]}"
