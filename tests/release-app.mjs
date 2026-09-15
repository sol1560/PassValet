import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync } from 'node:fs';
import path from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { ipcCall } from './ipc-client.mjs';

assert.equal(process.platform, 'darwin', '正式应用启动测试需要macOS');
const bundle = path.resolve(process.argv[2] ?? 'target/release/bundle/macos/PassValet.app');
const executable = execFileSync('/usr/libexec/PlistBuddy', ['-c', 'Print :CFBundleExecutable',
  path.join(bundle, 'Contents/Info.plist')], { encoding: 'utf8' }).trim();
const cli = path.join(bundle, 'Contents/MacOS/passvalet');
assert.match(execFileSync(cli, ['--version'], { encoding: 'utf8' }), /passvalet/i);

const home = mkdtempSync('/tmp/passvalet-release-');
const socket = path.join(home, 'passvalet.sock');
const app = spawn(path.join(bundle, 'Contents/MacOS', executable), [], {
  env: { ...process.env, PASSVALET_HOME: home, PASSVALET_SOCKET: socket,
    PASSVALET_DEV_SKIP_PRESENCE: '1', PASSVALET_LOG: 'warn' },
  stdio: 'ignore',
});
let startupError;
app.on('error', (error) => { startupError = error; });
const closed = new Promise((resolve) => app.once('close', resolve));
try {
  for (let attempt = 0; attempt < 100 && !existsSync(socket); attempt++) {
    assert.equal(startupError, undefined);
    assert.equal(app.exitCode, null, `正式应用提前退出，退出码 ${app.exitCode}`);
    assert.equal(app.signalCode, null, `正式应用被 ${app.signalCode} 终止`);
    await delay(200);
  }
  assert.ok(existsSync(socket), '正式应用应启动本地通信');
  const before = await ipcCall(socket, 'status');
  assert.equal(before.result?.vault.initialized, false);
  for (const method of ['dev.setup', 'dev.unlock', 'dev.decide', 'dev.add_secret']) {
    const response = await ipcCall(socket, method);
    assert.equal(response.error?.code, -32601, `正式版不得提供 ${method}`);
  }
  const after = await ipcCall(socket, 'status');
  assert.equal(after.result?.vault.initialized, false);
  assert.equal(readdirSync(home).some((name) => /^dev-kek.*\.bin$/.test(name)), false);
  mkdirSync('test-results', { recursive: true });
  // 通信启动不代表 WKWebView 已完成首次绘制；截图仍需单独检查。
  await delay(5000);
  assert.equal(app.exitCode, null, '等待页面绘制时应用不应退出');
  execFileSync('/usr/sbin/screencapture', ['-x', 'test-results/release-startup.png']);
  console.log('PASS: bundled CLI runs; release app starts; dev commands rejected; no dev key file');
} finally {
  app.kill('SIGTERM');
  await Promise.race([closed, delay(3000)]);
  if (app.exitCode === null && app.signalCode === null) app.kill('SIGKILL');
  await closed;
  rmSync(home, { recursive: true, force: true });
}
