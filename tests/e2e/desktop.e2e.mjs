import assert from 'node:assert/strict';
import { browser, $ } from '@wdio/globals';
import { McpClient, body } from './mcp-client.mjs';

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
    await $('.service-head .name=e2e').waitForDisplayed();
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
});
