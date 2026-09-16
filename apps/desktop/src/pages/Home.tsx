import { useEffect, useState } from "react";
import { api, errorText, fmtTime, onVaultChanged } from "../lib/api";
import { AUDIT_LIMIT, recentUsage } from "../lib/overview";
import type { AuditEntry, VaultInfo } from "../lib/types";

export default function Home({ info }: { info: VaultInfo }) {
  const [data, setData] = useState<{ entries: AuditEntry[]; info: VaultInfo; now: number } | null>(null);
  const [error, setError] = useState("");
  useEffect(() => {
    let alive = true;
    const load = async () => {
      try {
        const [entries, current] = await Promise.all([api.auditLog(AUDIT_LIMIT), api.vaultInfo()]);
        if (alive) { setData({ entries, info: current, now: Date.now() }); setError(""); }
      } catch (e) { if (alive) setError(errorText(e)); }
    };
    load();
    const un = onVaultChanged(load);
    const timer = setInterval(load, 30000);
    return () => { alive = false; clearInterval(timer); un.then((u) => u()); };
  }, [info.locked]);
  const usage = data && recentUsage(data.entries, data.now);
  return <div className="home">
    <div className="page-head"><div><h1>使用概况</h1><p className="muted">查看本机密钥与 Agent 的近期使用情况。</p></div><span className="tag ok">仅本机存储</span></div>
    {info.locked && <div className="notice">保险库已锁定。仅显示汇总数量，最近使用者在解锁后可见。</div>}
    {error ? <div className="notice danger">概况暂时无法更新：{error}</div> : !data || !usage ? <p className="muted">正在读取本机记录…</p> : <>
      <div className="stats">
        <div className="card"><span className="muted">已保存密钥</span><strong>{data.info.secret_count}</strong><small>当前保险库 · 含采集模型密钥</small></div>
        <div className="card"><span className="muted">当前会话</span><strong>{data.info.active_session_count}</strong><small>未过期且未撤销</small></div>
        <div className="card"><span className="muted">成功读取</span><strong>{usage.count}</strong><small>最近 7 天 · Agent 会话读取</small></div>
      </div>
      <p className="scope">{fmtTime(new Date(usage.days[0].start).toISOString())} 至 {fmtTime(new Date(data.now).toISOString())} · {Intl.DateTimeFormat().resolvedOptions().timeZone}。仅统计最近 {AUDIT_LIMIT} 条日志内的成功会话读取，不含显示、复制、拒绝或授权动作。{usage.limited && "已达到日志上限，次数可能不完整。"}</p>
      <section className="card"><div className="row between"><h2>成功读取趋势</h2><span className="muted">最近 7 个自然日（含今天）</span></div>
        <div className="trend" role="img" aria-label={usage.days.map((d) => `${d.label}：${d.count} 次`).join("，")}>
          {usage.days.map((d) => <div className="trend-day" key={d.start}><span>{d.count}</span><div className="trend-track"><div style={{ height: `${d.count / Math.max(1, ...usage.days.map((v) => v.count)) * 100}%` }} /></div><span>{d.label}</span></div>)}
        </div>
      </section>
      <section className="card recent"><div className="row between"><h2>最近使用者</h2><span className="muted">最近 7 天 · 最多 5 次</span></div>
        {info.locked ? <div className="empty"><h2>使用者信息已隐藏</h2><p>从左侧解锁后查看。</p></div> : usage.recent.length === 0 ? <div className="empty"><h2>这段记录中还没有成功读取</h2><p>Agent 获得授权并成功读取后，会在这里留下记录。</p></div> : usage.recent.map((e) => <div className="list-item" key={e.id}><span className="agent-avatar">↗</span><div className="grow"><strong>{e.agent || "未记录名称的 Agent"}</strong><div className="muted">成功读取密钥</div></div><time className="muted">{fmtTime(e.ts)}</time></div>)}
      </section>
      <p className="scope">此页不展示密钥值、指纹、项目路径或请求正文。</p>
    </>}
  </div>;
}
