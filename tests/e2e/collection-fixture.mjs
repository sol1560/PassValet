import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdir, writeFile } from 'node:fs/promises';
import path from 'node:path';

// 只替换外部网站和模型；Chrome、扩展、native host和保险库仍是真实实现。
export async function collectionFixture() {
  const secret = 'sk-passvalet-fixture-only-0123456789abcdefgh';
  let requests = 0;
  let failure;
  let stalled = false;
  let modelWaiting = false;
  const server = createServer(async (req, res) => {
    try {
      if (req.method === 'GET' && req.url === '/keys') {
        res.writeHead(200, { 'content-type': 'text/html; charset=utf-8' });
        res.end(`<title>Test keys</title><label>API key<input aria-label="API key" readonly value="${secret}"></label>`);
        return;
      }
      if (req.method !== 'POST' || req.url !== '/v1/chat/completions') {
        res.writeHead(404).end();
        return;
      }
      let raw = '';
      for await (const chunk of req) raw += chunk;
      assert.equal(raw.includes(secret), false, '发给模型的请求不能包含测试密钥');
      if (stalled) {
        modelWaiting = true;
        return; // 故意不响应，用真实未完成的HTTP请求测试中止。
      }
      const body = JSON.parse(raw);
      const tools = body.messages.filter((message) => message.role === 'tool');
      requests++;
      let name;
      let args;
      if (requests === 1) {
        name = 'read_page';
        args = { filter: 'interactive' };
      } else if (requests === 2) {
        const page = tools.at(-1)?.content ?? '';
        const ref = page.match(/\[(ref_\d+)\] textbox "API key"/)?.[1];
        assert.ok(ref, '真实扩展应返回密钥输入框的元素编号');
        assert.ok(page.includes('REDACTED'), '网页中的测试密钥应已隐藏');
        name = 'capture_secret';
        args = { key_type: 'api_key', source: 'element', ref };
      } else {
        assert.equal(requests, 3, '采集不应重复或无限重试');
        assert.ok(tools.at(-1)?.content.includes('format OK'), '保险库确认保存后才结束');
        name = 'done';
        args = { summary: '测试密钥已保存' };
      }
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ choices: [{ finish_reason: 'tool_calls', message: {
        role: 'assistant', content: null, tool_calls: [{
          id: `fixture_${requests}`, type: 'function',
          function: { name, arguments: JSON.stringify(args) },
        }],
      } }] }));
    } catch (error) {
      failure = error;
      res.writeHead(400).end('Test fixture rejected request');
    }
  });
  await new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen(0, '127.0.0.1', resolve);
  });
  const base = `http://127.0.0.1:${server.address().port}`;
  const dir = path.join(process.env.PASSVALET_HOME, 'playbooks');
  await mkdir(dir, { recursive: true });
  await writeFile(path.join(dir, 'collection-test.toml'), `
service = "collection_test"
label = "采集测试"
start_url = "${base}/keys"
key_types = ["api_key"]
max_steps = 8
[collect]
instructions = "Read the API key input and capture it."
`);
  return {
    provider: { kind: 'openai_compat', base_url: `${base}/v1`, models: ['test-model'] },
    stall() { stalled = true; },
    get modelWaiting() { return modelWaiting; },
    verify(value) {
      if (failure) throw failure;
      assert.equal(requests, 3);
      assert.ok(value === secret, '保存后读回的值必须与网页测试值完全一致');
    },
    async close() {
      server.closeAllConnections();
      await new Promise((resolve) => server.close(resolve));
    },
  };
}
