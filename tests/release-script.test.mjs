import assert from 'node:assert/strict';
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

for (const [name, zipFails, missingArtifacts, succeeds] of [
  ['zip failure stops the build', true, false, false],
  ['missing bundles fail the build', false, true, false],
  ['all required artifacts succeed', false, false, true],
]) {
  test(name, () => {
    const root = mkdtempSync(path.join(tmpdir(), 'passvalet-release-test-'));
    try {
      mkdirSync(path.join(root, 'scripts'));
      mkdirSync(path.join(root, 'bin'));
      copyFileSync('scripts/build-release.sh', path.join(root, 'scripts/build-release.sh'));
      const stub = (name, body) => writeFileSync(path.join(root, 'bin', name), `#!/bin/bash\nset -eu\n${body}\n`, { mode: 0o755 });
      stub('rustc', 'echo "host: aarch64-apple-darwin"');
      stub('cargo', 'mkdir -p target/release; printf "#!/bin/sh\\nexit 0\\n" > target/release/passvalet; chmod +x target/release/passvalet');
      stub('pnpm', `
if [[ "$*" == *" zip" && "$MOCK_ZIP_FAIL" == 1 ]]; then exit 7; fi
if [[ "$MOCK_MISSING" == 0 ]]; then
  mkdir -p target/release/bundle/macos/PassValet.app/Contents/MacOS target/release/bundle/dmg apps/extension/.output
  touch target/release/bundle/dmg/PassValet.dmg apps/extension/.output/passvalet-chrome.zip
  cp target/release/passvalet target/release/bundle/macos/PassValet.app/Contents/MacOS/passvalet
  cp target/release/passvalet target/release/bundle/macos/PassValet.app/Contents/MacOS/passvalet-desktop
fi`);
      const result = spawnSync('bash', [path.join(root, 'scripts/build-release.sh')], {
        cwd: root,
        env: { ...process.env, PATH: `${path.join(root, 'bin')}:${process.env.PATH}`,
          MOCK_ZIP_FAIL: zipFails ? '1' : '0', MOCK_MISSING: missingArtifacts ? '1' : '0' },
        encoding: 'utf8',
      });
      assert.equal(result.error, undefined);
      assert.equal(result.status === 0, succeeds, result.stdout + result.stderr);
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}
