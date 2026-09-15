import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import test from 'node:test';

function installTest(check) {
  const home = mkdtempSync(path.join(tmpdir(), 'passvalet-install-'));
  const cli = path.join(home, 'passvalet "test"');
  const bin = path.join(home, 'bin');
  mkdirSync(bin);
  mkdirSync(path.join(home, '.cursor'));
  writeFileSync(cli, '#!/bin/sh\nexit 0\n');
  chmodSync(cli, 0o700);
  // 没指定测试CLI时不调用机器上的Claude；真实CLI也只能使用临时配置目录。
  writeFileSync(path.join(bin, 'claude'), '#!/bin/sh\nexit 0\n');
  chmodSync(path.join(bin, 'claude'), 0o700);
  const config = path.join(home, '.cursor', 'mcp.json');
  const run = () => spawnSync('bash', ['scripts/install-mcp.sh', cli], {
    encoding: 'utf8', env: {
      ...process.env, HOME: home, CODEX_HOME: path.join(home, '.codex'),
      CLAUDE_CONFIG_DIR: path.join(home, '.claude'),
      PATH: `${process.env.PASSVALET_TEST_CLAUDE_BIN || bin}:${process.env.PASSVALET_TEST_CODEX_BIN || bin}:${bin}:${process.env.PATH}`,
    },
  });
  try {
    check({ config, cli, run, home, bin });
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

test('真实Claude安装后其他项目也能使用，重复安装保留其他服务', {
  skip: !process.env.PASSVALET_TEST_CLAUDE_BIN,
}, () => {
  installTest(({ cli, run, home }) => {
    const configDir = path.join(home, '.claude');
    mkdirSync(configDir);
    const config = path.join(configDir, '.claude.json');
    const previous = { mcpServers: { other: { type: 'stdio', command: 'other-tool', args: [] } }, custom: true };
    writeFileSync(config, JSON.stringify(previous));
    for (let i = 0; i < 2; i++) {
      const result = run();
      assert.equal(result.status, 0, result.stderr);
      const saved = JSON.parse(readFileSync(config, 'utf8'));
      assert.equal(saved.mcpServers.passvalet?.command, cli, '必须写入用户范围，不是当前项目');
      assert.deepEqual(saved.mcpServers.passvalet.args, ['mcp']);
      assert.deepEqual(saved.mcpServers.other, previous.mcpServers.other);
      assert.equal(saved.custom, true);
    }
  });
});

test('Claude注册失败时安装不能返回成功', { skip: !!process.env.PASSVALET_TEST_CLAUDE_BIN }, () => {
  installTest(({ run, bin }) => {
    writeFileSync(path.join(bin, 'claude'), '#!/bin/sh\nexit 42\n');
    assert.notEqual(run().status, 0);
  });
});

test('真实Codex首次安装、更新旧路径及损坏配置保护', {
  skip: !process.env.PASSVALET_TEST_CODEX_BIN,
}, () => {
  installTest(({ cli, run, home }) => {
    const config = path.join(home, '.codex', 'config.toml');
    const get = (name) => {
      const result = spawnSync(path.join(process.env.PASSVALET_TEST_CODEX_BIN, 'codex'),
        ['mcp', 'get', name, '--json'], {
          encoding: 'utf8', env: { ...process.env, HOME: home, CODEX_HOME: path.dirname(config) },
        });
      assert.equal(result.status, 0, result.stderr);
      return JSON.parse(result.stdout);
    };
    let result = run();
    assert.equal(result.status, 0, result.stderr);
    assert.equal(get('passvalet').transport.command, cli);
    writeFileSync(config, 'model = "gpt-5"\n[mcp_servers.other]\ncommand = "other-tool"\n[mcp_servers.passvalet]\ncommand = "old-path"\nargs = ["mcp"]\n');
    for (let i = 0; i < 2; i++) {
      result = run();
      assert.equal(result.status, 0, result.stderr);
      assert.equal(get('passvalet').transport.command, cli);
      assert.deepEqual(get('passvalet').transport.args, ['mcp']);
      assert.equal(get('other').transport.command, 'other-tool');
      assert.ok(readFileSync(config, 'utf8').includes('model = "gpt-5"'));
    }
    const broken = '[mcp_servers.other\n';
    writeFileSync(config, broken);
    result = run();
    assert.notEqual(result.status, 0);
    assert.equal(readFileSync(config, 'utf8'), broken);
  });
});
