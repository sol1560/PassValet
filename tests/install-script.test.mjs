import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

function installTest(check) {
  const home = mkdtempSync(path.join(tmpdir(), 'passvalet-install-'));
  const cli = path.join(home, 'passvalet');
  const bin = path.join(home, 'bin');
  mkdirSync(bin);
  mkdirSync(path.join(home, '.cursor'));
  writeFileSync(cli, '#!/bin/sh\nexit 0\n');
  chmodSync(cli, 0o700);
  // 不调用机器上真实的Claude配置命令。
  writeFileSync(path.join(bin, 'claude'), '#!/bin/sh\nexit 0\n');
  chmodSync(path.join(bin, 'claude'), 0o700);
  const config = path.join(home, '.cursor', 'mcp.json');
  const run = () => spawnSync('bash', ['scripts/install-mcp.sh', cli], {
    encoding: 'utf8', env: { ...process.env, HOME: home, PATH: `${bin}:${process.env.PATH}` },
  });
  try {
    check({ config, cli, run });
  } finally {
    rmSync(home, { recursive: true, force: true });
  }
}

test('安装和重复安装保留已有Cursor服务与其他设置', () => {
  installTest(({ config, cli, run }) => {
    const previous = { mcpServers: { other: { command: 'other-tool' } }, custom: true };
    writeFileSync(config, JSON.stringify(previous));
    for (let i = 0; i < 2; i++) {
      const result = run();
      assert.equal(result.status, 0, result.stderr);
      assert.deepEqual(JSON.parse(readFileSync(config, 'utf8')), {
        ...previous, mcpServers: { ...previous.mcpServers, passvalet: { command: cli, args: ['mcp'] } },
      });
      assert.equal(statSync(config).mode & 0o777, 0o600);
      assert.deepEqual(readdirSync(path.dirname(config)), ['mcp.json'], '不留下临时配置文件');
    }
  });
});

test('已有Cursor配置损坏时安装失败而不是清空原文件', () => {
  installTest(({ config, run }) => {
    const previous = '{"mcpServers": {"other":';
    writeFileSync(config, previous);
    const result = run();
    assert.notEqual(result.status, 0, '无效配置必须拒绝写入');
    assert.equal(readFileSync(config, 'utf8'), previous);
  });
});
