import assert from 'node:assert/strict';
import test from 'node:test';
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
