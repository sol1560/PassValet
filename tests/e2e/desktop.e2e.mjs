import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
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
      await $('button=我已完成，继续').waitForDisplayed();
      assert.ok((await $('.notice').getText()).includes('完成登录后继续'));
      assert.ok((await $('.notice').getText()).includes('REDACTED'));
      assert.equal((await $('body').getText()).includes(`ghp_${'A1'.repeat(20)}`), false,
        '模型返回的说明和任务日志不能显示完整示例令牌');
      await browser.saveScreenshot('test-results/collection-waiting.png');
      assert.equal(fixture.requestCount, 1, '用户继续之前不应再请求模型');
      const beforeResume = await browser.tauri.execute(({ core }) => core.invoke('reveal_secret', {
        service: 'collection_test', keyType: 'api_key',
      }));
      assert.ok(beforeResume === value, '暂停期间原密钥仍然可用');
      await $('button=我已完成，继续').click();
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
      fixture.stall();
      await $('button=在浏览器中开始采集 采集测试').click();
      await browser.waitUntil(() => fixture.modelWaiting, { timeout: 10_000 });
      const disconnectedRun = (await browser.tauri.execute(({ core }) => core.invoke('list_runs')))
        .find((r) => r.service === 'collection_test' && !r.finished)?.run_id;
      assert.ok(disconnectedRun, '断线前必须确实存在正在等待模型的任务');
      // 普通客户端的短连接结束不能中止仍连接着Chrome的任务。
      assert.equal((await ipcCall(process.env.PASSVALET_SOCKET, 'status', {})).result.extension_connected, true);
      assert.equal((await browser.tauri.execute(({ core }) => core.invoke('list_runs')))
        .find((r) => r.run_id === disconnectedRun).finished, null);
      await chrome.deleteSession();
      chrome = undefined;
      await browser.waitUntil(async () => {
        const runs = await browser.tauri.execute(({ core }) => core.invoke('list_runs'));
        return runs.find((r) => r.run_id === disconnectedRun)?.finished?.status === 'aborted';
      }, { timeout: 5_000, timeoutMsg: 'Chrome断线后应及时中止，不应等待模型超时' });
      fixture.verify(await browser.tauri.execute(({ core }) => core.invoke('reveal_secret', {
        service: 'collection_test', keyType: 'api_key',
      })));
      chrome = await openExtension();
      await $('span=扩展已连接').waitForDisplayed();
      const previousRuns = (await browser.tauri.execute(({ core }) => core.invoke('list_runs'))).map((r) => r.run_id);
      fixture.copyOnce();
      await $('button=在浏览器中开始采集 采集测试').click();
      await $('button=我已完成，继续').waitForDisplayed();
      await $('button=我已完成，继续').click();
      await browser.waitUntil(async () => {
        const runs = await browser.tauri.execute(({ core }) => core.invoke('list_runs'));
        run = runs.find((r) => !previousRuns.includes(r.run_id));
        return Boolean(run?.finished);
      }, { timeout: 45_000, timeoutMsg: 'Chrome重连后不能重新完成采集' });
      assert.equal(run.finished.status, 'success', JSON.stringify(run.finished));
      fixture.verify(await browser.tauri.execute(({ core }) => core.invoke('reveal_secret', {
        service: 'collection_test', keyType: 'api_key',
      })));
      fixture.stall();
      const starts = await Promise.all(Array.from({ length: 16 }, () =>
        ipcCall(process.env.PASSVALET_SOCKET, 'start_collection', {
          service: 'collection_test', key_types: ['api_key'],
        })));
      const accepted = starts.filter((reply) => reply.result?.run_id);
      try {
        assert.equal(accepted.length, 1, '同时请求采集只能启动一个任务');
        assert.equal(starts.filter((reply) => reply.error?.message.includes('已有一个采集任务')).length, 15);
        await browser.waitUntil(() => fixture.modelWaiting, { timeout: 10_000 });
      } finally {
        for (const reply of accepted) {
          await browser.tauri.execute(({ core }, runId) => core.invoke('abort_run', { runId }), reply.result.run_id);
        }
        await browser.waitUntil(async () => (await browser.tauri.execute(({ core }) => core.invoke('list_runs')))
          .every((r) => r.finished), { timeout: 5_000 });
      }
      fixture.finishWithoutCapture();
      for (const rotation of [false, true]) {
        const pending = rotation
          ? mcp.call('report_key_invalid', { service: 'collection_test', key_type: 'api_key', status_code: 401 })
          : mcp.call('request_permissions', {
            purpose: '测试未完成轮换不能返回旧密钥',
            requests: [{ service: 'collection_test', key_type: 'api_key', access: 'read_write' }],
            ttl_seconds: 300,
          });
        pending.catch(() => {});
        await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1);
        await browser.switchToWindow((await browser.getWindowHandles()).find((handle) => handle !== mainWindow));
        await $(rotation ? 'button=批准并轮换' : 'button=批准').waitForDisplayed();
        if (rotation) {
          assert.ok((await $('.notice.warn').getText()).includes('不会创建或撤销密钥'));
          await browser.saveScreenshot('test-results/rotation-blocked-prompt.png');
        }
        await $(rotation ? 'button=批准并轮换' : 'button=批准').click();
        const result = body(await pending);
        await browser.switchToWindow(mainWindow);
        if (rotation) {
          assert.equal(result.outcome, 'failed', '未实现安全撤销时不能执行轮换');
          assert.equal(result.value, null, '轮换失败不能把旧密钥当成新值返回');
          assert.ok(result.message.includes('安全自动轮换'));
          assert.equal(fixture.requestCount, 0, '危险轮换不能交给模型执行');
        } else {
          assert.equal(result.status, 'approved');
        }
      }
      const preserved = await browser.tauri.execute(({ core }) => core.invoke('reveal_secret', {
        service: 'collection_test', keyType: 'api_key',
      }));
      assert.ok(preserved === copied, '未完成的轮换应保留旧值');
    } finally {
      if (chrome) await chrome.deleteSession();
      await fixture.close();
      await browser.tauri.execute(({ core }, provider) => core.invoke('settings_set', { patch: { provider } }), settings.provider);
    }
  });

  it('failed-settings-save-keeps-the-active-configuration', async () => {
    const previous = await browser.tauri.execute(({ core }) => core.invoke('settings_get'));
    const file = path.join(process.env.PASSVALET_HOME, 'settings.json');
    const backup = `${file}.test-backup`;
    renameSync(file, backup);
    mkdirSync(file);
    try {
      const error = await browser.execute(async (minutes) => {
        try {
          await window.__TAURI__.core.invoke('settings_set', { patch: { auto_lock_minutes: minutes } });
          return null;
        } catch (error) { return String(error); }
      }, previous.auto_lock_minutes + 1);
      assert.ok(error, '模拟写入失败应返回错误');
      const current = await browser.tauri.execute(({ core }) => core.invoke('settings_get'));
      assert.equal(current.auto_lock_minutes, previous.auto_lock_minutes, '保存失败不能改变正在使用的设置');
    } finally {
      rmSync(file, { recursive: true });
      renameSync(backup, file);
    }
    assert.equal(readdirSync(process.env.PASSVALET_HOME).some((name) => /^settings\..*\.tmp$/.test(name)), false);
    const inode = statSync(file).ino;
    try {
      await browser.tauri.execute(({ core }, minutes) => core.invoke('settings_set', {
        patch: { auto_lock_minutes: minutes },
      }), previous.auto_lock_minutes + 1);
      assert.equal(JSON.parse(readFileSync(file, 'utf8')).auto_lock_minutes, previous.auto_lock_minutes + 1);
      assert.notEqual(statSync(file).ino, inode, '先写完临时文件再替换，不能原地截断配置');
      assert.equal(statSync(file).mode & 0o777, 0o600);
    } finally {
      await browser.tauri.execute(({ core }, minutes) => core.invoke('settings_set', {
        patch: { auto_lock_minutes: minutes },
      }), previous.auto_lock_minutes);
    }
  });

  it('copy-key-writes-the-real-macos-clipboard', async () => {
    await $('button=密钥').click();
    const card = await $('//div[contains(@class,"card")][.//span[@class="name" and text()="e2e"]]');
    await card.waitForDisplayed();
    execFileSync('pbcopy', [], { input: 'passvalet-clipboard-before-test' });
    try {
      await card.$('button=复制').click();
      await browser.waitUntil(() => execFileSync('pbpaste').toString() === secret, {
        timeout: 5_000, timeoutMsg: '复制按钮没有把选中的密钥写入系统剪贴板',
      });
      assert.equal((await card.$('.fp').getText()).includes(secret), false, '复制不应同时显示明文');
    } finally {
      execFileSync('pbcopy', [], { input: '' });
    }
  });

  it('cancel-add-and-filter-audit-without-showing-secrets', async () => {
    await $('button=手动添加').click();
    assert.equal(await $('button=保存').isEnabled(), false);
    await $('button=取消').click();
    await browser.waitUntil(async () => !(await $('textarea').isExisting()));
    await $('button=访问日志').click();
    const filter = await $('input[placeholder="筛选：agent / 服务 / 事件"]');
    await filter.setValue('e2e');
    await browser.waitUntil(() => browser.execute(() => {
      const rows = [...document.querySelectorAll('tbody tr')];
      return rows.length > 0 && rows.every((row) => row.textContent.toLowerCase().includes('e2e'));
    }));
    assert.equal((await $('table').getText()).includes(secret), false);
    await filter.setValue('no-such-audit-entry-for-test');
    await $('p=没有记录。').waitForDisplayed();
    await filter.setValue('e2e');
    await browser.waitUntil(async () => (await $('tbody tr').isExisting()));
    await browser.waitUntil(async () => !(await $('.toast').isExisting()));
    await browser.saveScreenshot('test-results/audit-filter.png');
  });

  it('delete-requires-confirmation-and-cancel-preserves-the-key', async () => {
    await $('button=密钥').click();
    const selector = '//div[contains(@class,"card")][.//span[@class="name" and text()="e2e"]]';
    for (const approve of [false, true]) {
      const card = await $(selector);
      await card.waitForDisplayed();
      const remove = await card.$('button=删除');
      const clicked = remove.click();
      clicked.catch(() => {});
      try {
        await browser.waitUntil(async () => {
          try { return (await browser.getAlertText()).includes('删除 e2e / api_key'); }
          catch { return false; }
        }, { timeout: 5_000, timeoutMsg: '删除前应确认具体服务与密钥类型' });
        if (approve) await browser.acceptAlert();
        else await browser.dismissAlert();
      } catch (error) {
        await browser.dismissAlert().catch(() => {});
        throw error;
      } finally {
        await clicked;
      }
      if (approve) {
        await browser.waitUntil(async () => !(await $(selector).isExisting()));
        const saved = await browser.tauri.execute(({ core }) => core.invoke('list_secrets'));
        assert.equal(saved.some((item) => item.service === 'e2e'), false);
      } else {
        const value = await browser.tauri.execute(({ core }) => core.invoke('reveal_secret', {
          service: 'e2e', keyType: 'api_key',
        }));
        assert.ok(value === secret, '取消删除后原密钥必须仍可读取');
      }
    }
  });

  it('settings-shows-user-wide-editor-configuration', async () => {
    const snippets = await browser.tauri.execute(({ core }) => core.invoke('mcp_config_snippet'));
    assert.ok(snippets.claude_code.startsWith('claude mcp add --scope user passvalet -- '));
    assert.equal(JSON.parse(snippets.cursor).mcpServers.passvalet.command, snippets.cli);
    assert.ok(snippets.codex.includes(`command = ${JSON.stringify(snippets.cli)}`));
    await $('button=设置').click();
    await $('h2=接入 Agent（MCP）').waitForDisplayed();
    await $('h2=接入 Agent（MCP）').scrollIntoView();
    assert.equal(await browser.execute(() => document.documentElement.scrollWidth <= window.innerWidth), true);
    await browser.saveScreenshot('test-results/settings-mcp-config.png');
  });

  it('idle-lock-hides-keys-and-unlock-preserves-their-values', async function () {
    this.timeout(130_000);
    const settings = await browser.tauri.execute(({ core }) => core.invoke('settings_get'));
    const before = await browser.tauri.execute(({ core }) => core.invoke('reveal_secret', {
      service: 'collection_test', keyType: 'api_key',
    }));
    const authorized = await mcp.call('get_key', { service: 'collection_test', key_type: 'api_key' });
    assert.ok(body(authorized).value === before, '自动锁定前客户端必须确实能读取同一密钥');
    try {
      await browser.tauri.execute(({ core }) => core.invoke('vault_unlock'));
      const started = Date.now();
      await browser.tauri.execute(({ core }) => core.invoke('settings_set', {
        patch: { auto_lock_minutes: 1 },
      }));
      await $('button=密钥').click();
      await browser.waitUntil(async () => (await browser.tauri.execute(({ core }) => core.invoke('vault_info'))).locked,
        { timeout: 95_000, interval: 1_000, timeoutMsg: '一分钟空闲后应由真实定时器锁定保险库' });
      assert.ok(Date.now() - started >= 59_000, '不能把一分钟误当成一秒或立即锁定');
      await $('h2=保险库已锁定').waitForDisplayed();
      assert.equal(await $('.fp').isExisting(), false);
      const denied = await mcp.call('get_key', { service: 'collection_test', key_type: 'api_key' });
      assert.equal(denied.isError, true, '已授权客户端也不能从锁定的保险库读取');
      assert.equal(body(denied).code, 'vault_locked');
      await browser.saveScreenshot('test-results/idle-locked.png');
      await browser.execute(() => [...document.querySelectorAll('button')]
        .find((button) => button.textContent.trim() === '用 Touch ID 解锁').focus());
      // embedded driver 的 keys() 只合成事件，不会触发原生按钮的 Enter 行为。
      await browser.execute(() => {
        document.addEventListener('keydown', (event) => {
          window.__passvaletKeyProbe = { key: event.key, trusted: event.isTrusted };
        }, { once: true });
      });
      const processes = execFileSync('ps', ['-A', '-o', 'pid=', '-o', 'comm='], { encoding: 'utf8' })
        .trim().split('\n').filter((line) => line.trim().endsWith(path.resolve('target/debug/passvalet-desktop')));
      assert.equal(processes.length, 1, '只向本轮测试应用发送按键');
      const pid = Number(processes[0].trim().split(/\s+/)[0]);
      execFileSync('osascript', [
        '-e', `tell application "System Events" to set frontmost of first application process whose unix id is ${pid} to true`,
        '-e', 'tell application "System Events" to key code 36',
      ], { timeout: 10_000 });
      await browser.waitUntil(() => browser.execute(() => window.__passvaletKeyProbe?.trusted === true));
      assert.equal(await browser.execute(() => window.__passvaletKeyProbe.key), 'Enter');
      await $('button=显示').waitForDisplayed();
      const after = await browser.tauri.execute(({ core }) => core.invoke('reveal_secret', {
        service: 'collection_test', keyType: 'api_key',
      }));
      assert.ok(after === before, '自动锁定再解锁不能改变已保存的密钥');
    } finally {
      await browser.tauri.execute(({ core }, minutes) => core.invoke('settings_set', {
        patch: { auto_lock_minutes: minutes },
      }), settings.auto_lock_minutes);
      await browser.tauri.execute(({ core }) => core.invoke('vault_unlock'));
    }
  });

  it('main-pages-fit-the-minimum-window-size', async () => {
    const original = await browser.getWindowSize();
    const config = JSON.parse(readFileSync('apps/desktop/src-tauri/tauri.conf.json', 'utf8'));
    const { minWidth, minHeight } = config.app.windows.find((window) => window.label === 'main');
    try {
      await browser.setWindowSize(minWidth, minHeight);
      await browser.waitUntil(() => browser.execute((width) => window.innerWidth <= width, minWidth));
      for (const [label, name] of [['密钥', 'keys'], ['自动采集', 'collect'], ['会话', 'sessions'], ['访问日志', 'audit'], ['设置', 'settings']]) {
        await $(`button=${label}`).click();
        await $(`h1=${label}`).waitForDisplayed();
        assert.equal(await browser.execute(() => document.documentElement.scrollWidth <= window.innerWidth), true,
          `${label}页面在最小窗口下不能横向溢出`);
        assert.equal(await browser.execute(() => {
          const main = document.querySelector('.main');
          return main.scrollWidth <= main.clientWidth;
        }), true, `${label}的内容不能藏在横向滚动区域内`);
        if (name === 'audit') {
          await $('tbody tr').waitForDisplayed();
          const widths = await browser.execute(() => [...document.querySelector('tbody tr').cells]
            .slice(2).map((cell) => cell.getBoundingClientRect().width));
          assert.ok(widths.every((width) => width >= 90), 'Agent、密钥和详情列必须留出可读宽度');
        }
        await browser.saveScreenshot(`test-results/minimum-${name}.png`);
      }
    } finally {
      await browser.setWindowSize(original.width, original.height);
    }
  });
});
