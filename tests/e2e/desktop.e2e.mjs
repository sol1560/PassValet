import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { renameSync, statSync } from 'node:fs';
import path from 'node:path';
import { browser, $ } from '@wdio/globals';
import { McpClient, body } from './mcp-client.mjs';
import { openExtension } from './chrome-extension.mjs';

const secret = 'passvalet-e2e-only-not-a-real-api-key';
let mcp;
let mainWindow;

describe('真实桌面与MCP', () => {
  after(() => mcp?.close());

  it('onboarding-keeps-recovery-until-confirmed', async () => {
    mainWindow = await browser.getWindowHandle();
    await $('button=创建保险库').waitForDisplayed();
    await $('button=创建保险库').click();
    await $('.recovery').waitForDisplayed();
    assert.ok((await $('.recovery').getText()).length > 20, '应显示恢复密钥');
    assert.equal(await $('button=进入 PassValet').isEnabled(), false);
    await browser.execute(() => { document.querySelector('.recovery').style.visibility = 'hidden'; });
    await browser.saveScreenshot('test-results/recovery-confirmation.png');
    await browser.execute(() => { document.querySelector('.recovery').style.visibility = ''; });
    await $('input[type="checkbox"]').click();
    await $('button=进入 PassValet').click();
    await $('button=手动添加').waitForDisplayed();
  });

  it('manual-add-persists-a-key', async () => {
    await $('button=手动添加').click();
    await $('button=自定义服务').click();
    await $('input[placeholder="例如 resend、posthog"]').setValue('e2e');
    await $('input[placeholder="api_key"]').setValue('api_key');
    await $('textarea').setValue(secret);
    await $('button=保存').click();
    await $('.service-head .name').waitForDisplayed();
    assert.equal(await $('.service-head .name').getText(), 'e2e');
    mcp = new McpClient();
    await mcp.init();
    const result = await mcp.call('get_key', { service: 'e2e', key_type: 'api_key' });
    assert.equal(result.isError, true);
    assert.equal(body(result).code, 'no_session');
  });

  it('hide-and-lock-clear-revealed-keys', async () => {
    await $('button=显示').click();
    await browser.waitUntil(async () => (await $('.fp').getText()).includes(secret));
    await $('button=隐藏').click();
    await browser.waitUntil(async () => !(await $('.fp').getText()).includes(secret));
    await $('button=显示').click();
    await browser.waitUntil(async () => (await $('.fp').getText()).includes(secret));
    await $('button=锁定').click();
    await $('h2=保险库已锁定').waitForDisplayed();
    assert.equal(await $('.fp').isExisting(), false);
    await $('button=用 Touch ID 解锁').click();
    await $('button=显示').waitForDisplayed();
    assert.equal((await $('.fp').getText()).includes(secret), false);
    await browser.saveScreenshot('test-results/keys-after-unlock.png');
  });

  it('approve-read-deny-ungranted-and-revoke', async () => {
    const pending = mcp.call('request_permissions', {
      purpose: '端到端测试：读取专用测试密钥',
      requests: [{ service: 'e2e', key_type: 'api_key', access: 'read' }],
      ttl_seconds: 60,
    });
    // 立即附上拒绝处理，UI失败时不留下未处理Promise。
    pending.catch(() => {});
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1);
    const prompt = (await browser.getWindowHandles()).find((handle) => handle !== mainWindow);
    await browser.switchToWindow(prompt);
    await $('button=批准').waitForDisplayed();
    await browser.saveScreenshot('test-results/authorization.png');
    await $('button=批准').click();
    assert.equal(body(await pending).status, 'approved');
    const key = body(await mcp.call('get_key', { service: 'e2e', key_type: 'api_key' }));
    assert.ok(key.value === secret, '授权读取应返回保存的测试值');
    const denied = await mcp.call('get_key', { service: 'e2e', key_type: 'other_key' });
    assert.equal(denied.isError, true);
    assert.equal(body(denied).code, 'not_granted');
    await browser.switchToWindow(mainWindow);
    await $('button=会话').click();
    await $('button=撤销').waitForDisplayed();
    await $('button=撤销').click();
    await browser.waitUntil(async () => !(await $('button=撤销').isExisting()));
    const revoked = await mcp.call('get_key', { service: 'e2e', key_type: 'api_key' });
    assert.equal(revoked.isError, true);
    assert.equal(body(revoked).code, 'session_revoked');
    await browser.saveScreenshot('test-results/session-revoked.png');
  });

  it('real-chrome-native-host-connects-and-disconnects', async function () {
    this.timeout(240_000);
    const chrome = await openExtension();
    try {
      await $('span=扩展已连接').waitForDisplayed();
    } finally {
      await chrome.deleteSession();
    }
    await $('span=扩展未连接').waitForDisplayed();
  });

  it('failed-rebind-keeps-the-original-unlock-key', async () => {
    const sql = (statement) => execFileSync('sqlite3', [
      '-cmd', '.timeout 5000', path.join(process.env.PASSVALET_HOME, 'vault.sqlite'), statement,
    ]);
    await $('button=设置').click();
    sql("CREATE TRIGGER fail_rebind BEFORE UPDATE ON vault_meta BEGIN SELECT RAISE(ABORT, 'test rebind metadata failure'); END;");
    try {
      await $('button=重新绑定 Touch ID').click();
      await browser.waitUntil(async () => (await $('.toast.error').getText()).includes('test rebind metadata failure'));
    } finally {
      sql('DROP TRIGGER IF EXISTS fail_rebind;');
    }
    await $('button=密钥').click();
    await $('button=锁定').click();
    await $('h2=保险库已锁定').waitForDisplayed();
    await $('button=用 Touch ID 解锁').click();
    await $('button=显示').waitForDisplayed();
    await $('button=显示').click();
    await browser.waitUntil(async () => (await $('.fp').getText()).includes(secret));
    await $('button=隐藏').click();
  });

  it('legacy-key-unlocks-and-successful-rebind-preserves-data', async () => {
    const home = process.env.PASSVALET_HOME;
    const salt = execFileSync('sqlite3', [path.join(home, 'vault.sqlite'),
      'SELECT lower(hex(kek_salt)) FROM vault_meta;']).toString().trim();
    const currentFile = path.join(home, `dev-kek-${salt}.bin`);
    assert.equal(statSync(currentFile).mode & 0o777, 0o600);
    renameSync(currentFile, path.join(home, 'dev-kek.bin'));
    await $('button=锁定').click();
    await $('button=用 Touch ID 解锁').click();
    await $('button=显示').waitForDisplayed();
    await $('button=设置').click();
    await $('button=重新绑定 Touch ID').click();
    await $('.recovery').waitForDisplayed();
    assert.ok((await $('.recovery').getText()).length > 20, '重新绑定应生成恢复密钥');
    await $('button=我已保存').click();
    await $('button=密钥').click();
    await $('button=锁定').click();
    await $('button=用 Touch ID 解锁').click();
    await $('button=显示').waitForDisplayed();
    await $('button=显示').click();
    await browser.waitUntil(async () => (await $('.fp').getText()).includes(secret));
    await $('button=隐藏').click();
  });

  it('regenerated-recovery-rejects-old-code-and-restores-data', async () => {
    await $('button=设置').click();
    await $('button=重新生成恢复密钥').click();
    await $('.recovery').waitForDisplayed();
    const oldRecovery = await $('.recovery').getText();
    await $('button=我已保存').click();
    await $('button=重新生成恢复密钥').click();
    await $('.recovery').waitForDisplayed();
    const newRecovery = await $('.recovery').getText();
    assert.ok(oldRecovery !== newRecovery, '恢复密钥应发生变化');
    await $('button=我已保存').click();
    await $('button=密钥').click();
    await $('button=锁定').click();
    await $('button=使用恢复密钥').click();
    await $('.onboard input').setValue(oldRecovery);
    await $('.onboard').$('button=解锁').click();
    await browser.waitUntil(() => browser.execute(() =>
      [...document.querySelectorAll('.toast.error')].some((el) => el.textContent.includes('invalid recovery key'))));
    assert.equal(await $('h2=保险库已锁定').isDisplayed(), true);
    await $('.onboard input').setValue(newRecovery);
    await $('.onboard').$('button=解锁').click();
    await $('button=显示').waitForDisplayed();
    await $('button=显示').click();
    await browser.waitUntil(async () => (await $('.fp').getText()).includes(secret));
    await $('button=隐藏').click();
  });
});
