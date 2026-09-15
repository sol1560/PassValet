import { spawn } from 'node:child_process';
import { createInterface } from 'node:readline';
import path from 'node:path';

// 真实stdio子进程。错误信息只包含方法，不输出工具结果里的密钥。
export class McpClient {
  #next = 0;
  #pending = new Map();
  #process;

  constructor() {
    this.#process = spawn(path.resolve('target/debug/passvalet'), ['mcp'], {
      env: process.env,
      stdio: ['pipe', 'pipe', 'ignore'],
    });
    createInterface({ input: this.#process.stdout }).on('line', (line) => {
      const message = JSON.parse(line);
      const pending = this.#pending.get(message.id);
      if (!pending) return;
      clearTimeout(pending.timer);
      this.#pending.delete(message.id);
      if (message.error) pending.reject(new Error('MCP请求失败'));
      else pending.resolve(message.result);
    });
    const rejectAll = () => {
      for (const pending of this.#pending.values()) {
        clearTimeout(pending.timer);
        pending.reject(new Error('MCP子进程结束'));
      }
      this.#pending.clear();
    };
    this.#process.on('error', rejectAll);
    this.#process.on('exit', rejectAll);
  }

  async init() {
    await this.request('initialize', {
      protocolVersion: '2025-03-26', capabilities: {},
      clientInfo: { name: 'PassValet E2E', version: '1.0.0' },
    });
    this.#process.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', method: 'notifications/initialized' })}\n`);
  }

  request(method, params) {
    const id = ++this.#next;
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.#pending.delete(id);
        reject(new Error(`MCP ${method} 超时`));
      }, 60_000);
      this.#pending.set(id, { resolve, reject, timer });
      this.#process.stdin.write(`${JSON.stringify({ jsonrpc: '2.0', id, method, params })}\n`);
    });
  }

  call(name, args = {}) {
    return this.request('tools/call', { name, arguments: args });
  }

  close() {
    this.#process.stdin.end();
    this.#process.kill('SIGTERM');
  }
}

export function body(result) {
  return result.structuredContent ?? JSON.parse(result.content.find((entry) => entry.type === 'text').text);
}
