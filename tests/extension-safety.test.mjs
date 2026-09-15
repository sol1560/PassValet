import assert from 'node:assert/strict';
import test from 'node:test';
import { runInNewContext } from 'node:vm';
import { Executor } from '../apps/extension/src/executor.ts';

test('网页确认和输入弹窗不能被自动同意', () => {
  let onDialog;
  const commands = [];
  globalThis.chrome = {
    tabs: { onRemoved: { addListener() {} } },
    debugger: {
      onEvent: { addListener(listener) { onDialog = listener; } },
      onDetach: { addListener() {} },
      sendCommand(source, method, params, callback) {
        commands.push({ source, method, params });
        callback();
      },
    },
    runtime: {},
  };
  try {
    new Executor();
    for (const type of ['confirm', 'prompt', 'alert']) {
      onDialog({ tabId: 1 }, 'Page.javascriptDialogOpening', {
        type, message: 'An operation needs attention', defaultPrompt: 'untrusted default',
      });
    }
    assert.deepEqual(commands.map((command) => command.params.accept), [false, false, true]);
    assert.equal(commands.some((command) => command.params.promptText === 'untrusted default'), false);
  } finally {
    delete globalThis.chrome;
  }
});

test('采集拒绝其他网站和非网页地址，包括跳转后的页面', async () => {
  const origin = 'https://dashboard.example.test';
  const tab = { id: 1, url: `${origin}/keys`, title: 'Keys', status: 'complete' };
  const commands = [];
  let created = 0;
  let redirect = false;
  let clipboardHook;
  globalThis.chrome = {
    tabs: {
      onRemoved: { addListener() {} },
      async create({ url }) { created++; tab.url = url; return { ...tab }; },
      async get() { return { ...tab }; },
      async group() { return 1; },
      async remove() {},
      async update(_id, { url }) { if (url) tab.url = redirect ? 'https://other.test/' : url; },
    },
    tabGroups: { async update() {} },
    debugger: {
      onEvent: { addListener() {} },
      onDetach: { addListener() {} },
      async attach() {},
      async detach() {},
      sendCommand(_source, method, params, callback) {
        commands.push(method);
        if (method === 'Page.addScriptToEvaluateOnNewDocument') clipboardHook = params.source;
        if (redirect && method === 'Input.dispatchMouseEvent' && params.type === 'mouseReleased') {
          tab.url = 'https://other.test/';
        }
        callback({ result: { value: 'safe page' }, nodes: [] });
      },
    },
    runtime: {},
  };
  const executor = new Executor();
  const wrongOrigin = (error) => error.data?.code === 'wrong_origin';
  try {
    for (const url of ['file:///tmp/example', 'javascript:void(0)', 'https://user:password@example.test']) {
      await assert.rejects(executor.sessionBegin('invalid', url), (error) => error.data?.code === 'unsafe_url');
    }
    assert.equal(created, 0, '不安全地址不能创建标签页');
    await executor.sessionBegin('run', `${origin}/keys`);
    for (const site of [origin, 'https://other.test']) {
      const context = { location: { origin: site }, window: {}, navigator: {}, document: { addEventListener() {} } };
      runInNewContext(clipboardHook, context);
      assert.equal(context.window.__pvHooked === true, site === origin, '复制记录脚本只在本次任务的网站安装');
    }
    const output = await executor.call('get_page_text', { run_id: 'run' });
    assert.equal(output.text, 'safe page', '允许读取本次任务的网站');
    await assert.rejects(executor.call('navigate', { run_id: 'run', url: 'https://other.test/keys' }), wrongOrigin);
    assert.equal(tab.url, `${origin}/keys`, '跨站导航不能发出请求');
    tab.url = 'https://other.test/';
    const before = commands.length;
    for (const tool of ['get_page_text', 'read_page', 'capture_secret', 'left_click']) {
      await assert.rejects(executor.call(tool, { run_id: 'run', coordinate: [10, 10] }), wrongOrigin);
    }
    assert.equal(commands.length, before, '跨站页面不执行读取或输入命令');
    tab.url = `${origin}/keys`;
    redirect = true;
    await assert.rejects(executor.call('navigate', { run_id: 'run', url: `${origin}/redirect` }), wrongOrigin);
    tab.url = `${origin}/keys`;
    await assert.rejects(executor.call('left_click', { run_id: 'run', coordinate: [10, 10] }), wrongOrigin);
  } finally {
    await executor.sessionEnd('run', false);
    delete globalThis.chrome;
  }
});
