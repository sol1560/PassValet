import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtempSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { browser, $ } from '@wdio/globals';
import { McpClient, body } from './mcp-client.mjs';
import { openExtension } from './chrome-extension.mjs';
import { collectionFixture } from './collection-fixture.mjs';
import { ipcCall } from '../ipc-client.mjs';

const secret = 'passvalet-e2e-only-not-a-real-api-key';
let mcp;
let mainWindow;

describe('真实桌面与MCP', () => {
  after(() => mcp?.close());

  it('onboarding-keeps-recovery-until-confirmed', async () => {
    mainWindow = await browser.getWindowHandle();
    await $('button=创建保险库').waitForDisplayed();
    for (const index of [1, 2]) {
      await $(`.choice button:nth-child(${index})`).click();
      assert.equal(await browser.execute(() => {
        const choice = document.querySelector('.choice');
        return choice.scrollWidth <= choice.clientWidth &&
          document.documentElement.scrollWidth <= window.innerWidth;
      }), true, '解锁选项应在窗口内完整显示，没有横向溢出');
      await browser.saveScreenshot(`test-results/setup-option-${index}.png`);
    }
    await $('.choice button:first-child').click();
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
      const response = await ipcCall(process.env.PASSVALET_SOCKET, 'ext.event', { kind: 'user_aborted', run_id: 'unrelated-peer-test' });
      assert.equal(response.error?.data?.code, 'not_extension');
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

  it('denied-request-does-not-grant-a-session', async () => {
    const client = new McpClient();
    try {
      await client.init();
      const pending = client.call('request_permissions', {
        purpose: '测试拒绝后不可读取',
        requests: [{ service: 'e2e', key_type: 'api_key', access: 'read' }],
      });
      pending.catch(() => {});
      await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1);
      const prompt = (await browser.getWindowHandles()).find((handle) => handle !== mainWindow);
      await browser.switchToWindow(prompt);
      await $('button=拒绝').waitForDisplayed();
      await $('button=拒绝').click();
      const denied = await pending;
      assert.equal(denied.isError, true);
      assert.equal(body(denied).code, 'user_denied');
      const read = await client.call('get_key', { service: 'e2e', key_type: 'api_key' });
      assert.equal(read.isError, true);
      assert.equal(body(read).code, 'no_session');
    } finally {
      await browser.switchToWindow(mainWindow);
      client.close();
    }
  });

  it('concurrent-clients-cannot-use-each-others-approval', async () => {
    const first = new McpClient();
    const second = new McpClient();
    try {
      await Promise.all([first.init(), second.init()]);
      const requests = [{ service: 'e2e', key_type: 'api_key', access: 'read' }];
      const firstPending = first.call('request_permissions', { purpose: '并发请求甲：应拒绝', requests });
      firstPending.catch(() => {});
      await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1);
      const prompt = (await browser.getWindowHandles()).find((handle) => handle !== mainWindow);
      await browser.switchToWindow(prompt);
      await browser.waitUntil(async () => (await $('.purpose').getText()).includes('并发请求甲：应拒绝'));
      const secondPending = second.call('request_permissions', { purpose: '并发请求乙：应批准', requests });
      secondPending.catch(() => {});
      await $('span=还有 1 个请求排队').waitForDisplayed();
      await $('button=拒绝').click();
      assert.equal(body(await firstPending).code, 'user_denied');
      await browser.waitUntil(async () => (await $('.purpose').getText()).includes('并发请求乙：应批准'));
      await $('button=批准').click();
      assert.equal(body(await secondPending).status, 'approved');
      const denied = await first.call('get_key', { service: 'e2e', key_type: 'api_key' });
      assert.equal(denied.isError, true);
      assert.equal(body(denied).code, 'no_session');
      const allowed = await second.call('get_key', { service: 'e2e', key_type: 'api_key' });
      assert.ok(body(allowed).value === secret, '批准只能允许对应客户端读取');
      await browser.switchToWindow(mainWindow);
      await $('button=会话').click();
      await $('button=撤销').waitForDisplayed();
      await $('button=撤销全部').click();
      await browser.waitUntil(async () => !(await $('button=撤销').isExisting()));
      const revoked = await second.call('get_key', { service: 'e2e', key_type: 'api_key' });
      assert.equal(revoked.isError, true);
      assert.equal(body(revoked).code, 'session_revoked');
    } finally {
      await browser.switchToWindow(mainWindow);
      first.close();
      second.close();
    }
  });

  it('cli-injection-preserves-project-data-and-reports-missing-keys', async () => {
    const dir = mkdtempSync('/tmp/passvalet-project-');
    try {
      for (const mode of ['complete', 'partial', 'missing']) {
        const requests = [];
        if (mode !== 'missing') requests.push({ service: 'e2e', key_type: 'api_key', env_var: 'TEST_TOKEN' });
        if (mode !== 'complete') requests.push({ service: 'e2e', key_type: 'missing_key', env_var: 'MISSING_TOKEN' });
        writeFileSync(path.join(dir, '.passvalet.json'), JSON.stringify({ project: 'cli-test', requests }));
        writeFileSync(path.join(dir, '.env.local'), 'KEEP=original\n');
        const previousInode = statSync(path.join(dir, '.env.local')).ino;
        const child = spawn(path.resolve('target/debug/passvalet'), ['inject'], { cwd: dir, env: process.env });
        let stdout = '';
        let stderr = '';
        child.stdout.on('data', (chunk) => { stdout += chunk; });
        child.stderr.on('data', (chunk) => { stderr += chunk; });
        const closed = new Promise((resolve, reject) => {
          child.once('error', reject);
          child.once('close', resolve);
        });
        closed.catch(() => {});
        try {
          await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1);
          const prompt = (await browser.getWindowHandles()).find((handle) => handle !== mainWindow);
          await browser.switchToWindow(prompt);
          await $('button=批准').waitForDisplayed();
          await $('button=批准').click();
          const code = await closed;
          assert.equal(code, mode === 'complete' ? 0 : 1, '有缺失项时CLI不能报成功');
          const contents = readFileSync(path.join(dir, '.env.local'), 'utf8');
          assert.ok(contents.includes('KEEP=original\n'), '保留原来的项目配置');
          if (mode !== 'missing') {
            assert.ok(contents.includes(`TEST_TOKEN=${secret}\n`), '写入经批准的测试密钥');
            assert.equal(statSync(path.join(dir, '.env.local')).mode & 0o777, 0o600);
          } else {
            assert.equal(contents, 'KEEP=original\n');
            assert.equal(statSync(path.join(dir, '.env.local')).ino, previousInode, '全部缺失时不替换文件');
          }
          assert.equal(stdout.includes(secret) || stderr.includes(secret), false, '普通注入不输出密钥');
          if (mode !== 'complete') assert.ok(stderr.includes('missing_key'), '指出缺失的密钥类型');
        } finally {
          child.kill('SIGTERM');
          await browser.switchToWindow(mainWindow);
        }
      }
    } finally {
      rmSync(dir, { recursive: true, force: true });
    }
  });

  it('timed-out-request-cannot-be-approved-later', async () => {
    const pending = ipcCall(process.env.PASSVALET_SOCKET, 'request_permissions', {
      manifest: { agent: { name: 'timeout-test' }, purpose: '测试过期后拒绝批准',
        requests: [{ service: 'e2e', key_type: 'api_key', access: 'read' }] },
      wait_seconds: 3,
    });
    pending.catch(() => {});
    let prompt;
    await browser.waitUntil(async () => {
      const prompts = await browser.tauri.execute(({ core }) => core.invoke('prompt_list'));
      prompt = prompts.find((p) => p.manifest.purpose === '测试过期后拒绝批准');
      return Boolean(prompt);
    });
    assert.equal((await pending).error?.data?.code, 'timeout');
    const error = await browser.execute(async (id) => {
      try {
        await window.__TAURI__.core.invoke('prompt_decide', { id, approve: true });
        return null;
      } catch (error) { return String(error); }
    }, prompt.id);
    assert.equal(error, '该请求已过期');
    const sessions = await browser.tauri.execute(({ core }) => core.invoke('list_sessions', { includeInactive: false }));
    assert.equal(sessions.some((s) => s.purpose === '测试过期后拒绝批准'), false);
  });

  it('real-extension-collects-without-sending-the-key-to-the-model', async function () {
    this.timeout(240_000);
    const fixture = await collectionFixture();
    const settings = await browser.tauri.execute(({ core }) => core.invoke('settings_get'));
    let chrome;
    try {
      await browser.tauri.execute(({ core }, provider) => core.invoke('settings_set', { patch: { provider } }), fixture.provider);
      chrome = await openExtension();
      await $('span=扩展已连接').waitForDisplayed();
      await $('button=自动采集').click();
      await $('button=采集测试').click();
      assert.equal(await browser.execute(() =>
        document.querySelector('.page-head p').textContent.includes('规则可能漏掉其他敏感内容') &&
        document.documentElement.scrollWidth <= window.innerWidth), true);
      await $('button=在浏览器中开始采集 采集测试').click();
      let run;
      await browser.waitUntil(async () => {
        const runs = await browser.tauri.execute(({ core }) => core.invoke('list_runs'));
        run = runs.find((r) => r.service === 'collection_test');
        return Boolean(run?.finished);
      }, { timeout: 45_000, timeoutMsg: '真实扩展采集没有结束' });
      assert.equal(run.finished.status, 'success', JSON.stringify(run.finished));
      const value = await browser.tauri.execute(({ core }) => core.invoke('reveal_secret', {
        service: 'collection_test', keyType: 'api_key',
      }));
      fixture.verify(value);
      await $('span=成功').waitForDisplayed();
      await browser.saveScreenshot('test-results/collection-success.png');
      const firstRun = run.run_id;
      fixture.copyOnce();
      await $('button=在浏览器中开始采集 采集测试').click();
      await browser.waitUntil(async () => {
        const runs = await browser.tauri.execute(({ core }) => core.invoke('list_runs'));
        run = runs.find((r) => r.service === 'collection_test' && r.run_id !== firstRun);
        return Boolean(run?.finished);
      }, { timeout: 45_000, timeoutMsg: '一次性显示的密钥未完成复制采集' });
      assert.equal(run.finished.status, 'success', JSON.stringify(run.finished));
      const copied = await browser.tauri.execute(({ core }) => core.invoke('reveal_secret', {
        service: 'collection_test', keyType: 'api_key',
      }));
      fixture.verify(copied);
      assert.ok(copied !== value, '复制采集不能复用上一次保存的值');
      await browser.waitUntil(() => browser.execute(() =>
        [...document.querySelectorAll('span.tag')].filter((el) => el.textContent === '成功').length === 2));
      await browser.saveScreenshot('test-results/collection-copy-once.png');
      fixture.stall();
      await $('button=在浏览器中开始采集 采集测试').click();
      await browser.waitUntil(() => fixture.modelWaiting, { timeout: 10_000 });
      await $('button=中止').click();
      await browser.waitUntil(async () => {
        const runs = await browser.tauri.execute(({ core }) => core.invoke('list_runs'));
        return runs.some((r) => r.service === 'collection_test' && r.run_id !== run.run_id && r.finished?.status === 'aborted');
      }, { timeout: 5_000, timeoutMsg: '中止不应等待模型的180秒超时' });
      fixture.verify(await browser.tauri.execute(({ core }) => core.invoke('reveal_secret', {
        service: 'collection_test', keyType: 'api_key',
      })));
      await $('span=已中止').waitForDisplayed();
      await browser.waitUntil(async () => !(await $('.toast').isExisting()));
      await browser.saveScreenshot('test-results/collection-aborted.png');
    } finally {
      if (chrome) await chrome.deleteSession();
      await fixture.close();
      await browser.tauri.execute(({ core }, provider) => core.invoke('settings_set', { patch: { provider } }), settings.provider);
    }
  });
});
