import assert from 'node:assert/strict';
import { createHash, generateKeyPairSync } from 'node:crypto';
import { cp, mkdir, mkdtemp, readFile, writeFile } from 'node:fs/promises';
import path from 'node:path';
import { remote } from 'webdriverio';

// 只改临时复制品中的公钥，确保测试扩展ID可预测；不接触用户Chrome目录。
export async function openExtension() {
  const home = process.env.PASSVALET_HOME;
  if (!home || !process.env.PASSVALET_SOCKET) throw new Error('扩展测试必须使用隔离目录');
  const sessionDir = await mkdtemp(path.join(home, 'chrome-test-'));
  const extension = path.join(sessionDir, 'extension');
  const profile = path.join(sessionDir, 'chrome-profile');
  await cp(path.resolve('apps/extension/.output/chrome-mv3'), extension, { recursive: true });
  const { publicKey } = generateKeyPairSync('rsa', { modulusLength: 2048 });
  const key = publicKey.export({ type: 'spki', format: 'der' });
  const id = createHash('sha256').update(key).digest('hex').slice(0, 32)
    .replace(/[0-9a-f]/g, (digit) => String.fromCharCode(97 + parseInt(digit, 16)));
  const manifestPath = path.join(extension, 'manifest.json');
  const manifest = JSON.parse(await readFile(manifestPath, 'utf8'));
  await writeFile(manifestPath, JSON.stringify({ ...manifest, key: key.toString('base64') }));
  const hosts = path.join(profile, 'NativeMessagingHosts');
  await mkdir(hosts, { recursive: true });
  await writeFile(path.join(hosts, 'ai.passvalet.host.json'), JSON.stringify({
    name: 'ai.passvalet.host',
    description: 'PassValet isolated end-to-end test',
    path: path.resolve('target/debug/passvalet'),
    type: 'stdio',
    allowed_origins: [`chrome-extension://${id}/`],
  }));
  const chrome = await remote({
    logLevel: 'error',
    connectionRetryCount: 0,
    capabilities: {
      browserName: 'chrome',
      browserVersion: '153.0.8010.36',
      'wdio:enforceWebDriverClassic': true,
      'goog:chromeOptions': {
        args: [`--user-data-dir=${profile}`, `--load-extension=${extension}`, '--no-first-run'],
      },
    },
  });
  try {
    await chrome.url(`chrome-extension://${id}/popup.html`);
    await chrome.waitUntil(async () => (await chrome.$('#status').getText()) === '已连接桌面 app', {
      timeout: 30_000,
      timeoutMsg: '真实Chrome扩展未连接native host和桌面应用',
    });
    assert.equal(await chrome.$('#extid').getProperty('textContent'), id);
    await chrome.saveScreenshot('test-results/chrome-extension-connected.png');
    return chrome;
  } catch (error) {
    await chrome.deleteSession();
    throw error;
  }
}
