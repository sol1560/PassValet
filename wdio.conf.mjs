import { access, mkdir } from 'node:fs/promises';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { browser } from '@wdio/globals';

const root = path.dirname(fileURLToPath(import.meta.url));
const home = process.env.PASSVALET_HOME;
if (!home || !process.env.PASSVALET_SOCKET) {
  throw new Error('测试必须设置隔离的 PASSVALET_HOME 和 PASSVALET_SOCKET');
}
const artifacts = path.join(root, 'test-results');
const binary = path.join(root, 'target/debug/passvalet-desktop');

export const config = {
  runner: 'local',
  specs: ['./tests/e2e/desktop.e2e.mjs'],
  maxInstances: 1,
  framework: 'mocha',
  mochaOpts: { timeout: 120_000, bail: true },
  reporters: ['spec'],
  logLevel: 'error',
  waitforTimeout: 15_000,
  connectionRetryTimeout: 60_000,
  connectionRetryCount: 0,
  services: [['@wdio/tauri-service', {
    driverProvider: 'embedded',
    startTimeout: 60_000,
    env: {
      PASSVALET_HOME: home,
      PASSVALET_SOCKET: process.env.PASSVALET_SOCKET,
      PASSVALET_DEV_SKIP_PRESENCE: '1',
      PASSVALET_LOG: 'warn',
    },
    captureBackendLogs: false,
    autoInstallTauriDriver: false,
    autoDownloadEdgeDriver: false,
  }]],
  capabilities: [{ browserName: 'tauri', 'tauri:options': { application: binary } }],
  onPrepare: async () => {
    await access(binary);
    await mkdir(artifacts, { recursive: true });
  },
  // 只记录命令名和时间，不记录参数、返回值或输入的密钥。
  beforeCommand: (name) => console.info(`[driver] ${Date.now()} ${name} start`),
  afterCommand: (name) => console.info(`[driver] ${Date.now()} ${name} end`),
  afterTest: async (test, _context, result) => {
    if (!result.passed) {
      // 只使用假密钥。遮住恢复码与密钥输入框，避免把测试习惯带入真实数据测试。
      await browser.execute(() => {
        for (const node of document.querySelectorAll('.recovery, textarea, input:not([type="checkbox"]), .fp')) {
          node.style.visibility = 'hidden';
        }
      });
      await browser.saveScreenshot(path.join(artifacts, `failed-${test.title.replace(/[^a-zA-Z0-9-]/g, '_')}.png`));
    }
  },
};
