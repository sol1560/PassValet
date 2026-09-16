import { test } from "node:test";
import assert from "node:assert/strict";
import { recentUsage } from "../src/lib/overview.ts";
import type { AuditEntry } from "../src/lib/types";

const now = new Date(2026, 8, 15, 14, 30).getTime();
const entry = (id: number, ts: number, patch: Partial<AuditEntry> = {}): AuditEntry => ({
  id, ts: new Date(ts).toISOString(), event: "key_read", session_id: "session-test", agent: "测试 Agent", ...patch,
});

test("只统计成功会话读取，排除手动查看、拒绝、批准和将来时间", () => {
  const result = recentUsage([
    entry(1, now), entry(2, now - 60000, { session_id: null }),
    entry(3, now, { event: "key_denied" }), entry(4, now, { event: "session_approved" }),
    entry(5, now + 1), entry(6, now - 120000, { detail: "private request", service: "private service" }),
  ], now);
  assert.equal(result.count, 2);
  assert.deepEqual(result.recent.map((e) => e.id), [1, 6]);
  assert.deepEqual(Object.keys(result.recent[1]).sort(), ["agent", "id", "ts"]);
});

test("自然日起点含当天零点，排除前一毫秒，跨日分别计数", () => {
  const start = new Date(2026, 8, 9).getTime();
  const next = new Date(2026, 8, 10).getTime();
  const result = recentUsage([entry(1, start - 1), entry(2, start), entry(3, next - 1), entry(4, next), entry(5, now)], now);
  assert.deepEqual(result.days.map((d) => d.count), [2, 1, 0, 0, 0, 0, 1]);
  assert.deepEqual(result.days.map((d) => d.label), ["9/9", "9/10", "9/11", "9/12", "9/13", "9/14", "9/15"]);
  assert.equal(result.count, 4);
});

test("空数据是真正的零；500 条上限明确标记且最近记录最多五条", () => {
  assert.equal(recentUsage([], now).count, 0);
  assert.deepEqual(recentUsage([], now).recent, []);
  assert.equal(recentUsage(Array.from({ length: 499 }, (_, i) => entry(i, now)), now).limited, false);
  const result = recentUsage(Array.from({ length: 500 }, (_, i) => entry(i, now - i * 1000)), now);
  assert.equal(result.limited, true);
  assert.equal(result.count, 500);
  assert.deepEqual(result.recent.map((e) => e.id), [0, 1, 2, 3, 4]);
});

test("跨月与夏令时仍按本机自然日，而非固定24小时偏移", () => {
  const spring = new Date(2026, 2, 10, 12).getTime();
  const days = recentUsage([], spring).days;
  assert.equal(days[0].start, new Date(2026, 2, 4).getTime());
  assert.equal(days[4].end, new Date(2026, 2, 9).getTime());
  const month = recentUsage([], new Date(2026, 0, 3, 12).getTime());
  assert.deepEqual(month.days.map((d) => d.label), ["12/28", "12/29", "12/30", "12/31", "1/1", "1/2", "1/3"]);
});
