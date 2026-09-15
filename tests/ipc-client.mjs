import { createConnection } from 'node:net';

// 只用于隔离测试；保留JSON-RPC错误，让调用方检查实际拒绝原因。
export function ipcCall(socketPath, method, params = {}) {
  return new Promise((resolve, reject) => {
    let data = '';
    const socket = createConnection(socketPath, () => {
      socket.write(`${JSON.stringify({ jsonrpc: '2.0', id: 1, method, params })}\n`);
    });
    socket.setEncoding('utf8');
    socket.setTimeout(10_000, () => socket.destroy(new Error('本地请求测试响应超时')));
    socket.on('error', reject);
    socket.on('end', () => reject(new Error('本地请求测试连接提前结束')));
    socket.on('data', (chunk) => {
      data += chunk;
      if (!data.includes('\n')) return;
      try { resolve(JSON.parse(data.slice(0, data.indexOf('\n')))); }
      catch (error) { reject(error); }
      finally { socket.destroy(); }
    });
  });
}
