import { useEffect, useState } from "react";
import { api, errorText, onPromptChanged } from "../lib/api";
import type { PendingPrompt } from "../lib/types";

export default function PromptWindow() {
  const [prompts, setPrompts] = useState<PendingPrompt[]>([]);
  const load = () => api.promptList().then(setPrompts).catch(() => {});
  useEffect(() => {
    load();
    const un = onPromptChanged(load);
    const t = setInterval(load, 2000);
    return () => { un.then((u) => u()); clearInterval(t); };
  }, []);

  const p = prompts[0];
  return (
    <div className="prompt">
      <div className="titlebar-drag" />
      {!p ? (
        <div className="center muted">没有待处理的请求</div>
      ) : (
        <>
          {prompts.length > 1 && <span className="tag">还有 {prompts.length - 1} 个请求排队</span>}
          <PromptCard prompt={p} onDone={load} compact />
        </>
      )}
    </div>
  );
}

export function PromptCard({ prompt, onDone, compact }: { prompt: PendingPrompt; onDone: () => void; compact?: boolean }) {
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const s = prompt.summary;
  const isRotate = prompt.kind.kind === "rotate";

  async function decide(approve: boolean) {
    setBusy(true);
    setErr(null);
    try {
      await api.promptDecide(prompt.id, approve);
      onDone();
    } catch (e) {
      setErr(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="col" style={{ height: compact ? "100%" : undefined }}>
      <div>
        <div className="muted" style={{ fontSize: 12 }}>{isRotate ? "请求轮换密钥" : "请求访问密钥"}</div>
        <div className="agent">{s.agent_label}</div>
      </div>
      <div className="purpose">
        <div className="muted" style={{ fontSize: 12, marginBottom: 4 }}>用来干什么</div>
        <div>{s.purpose}</div>
        {prompt.manifest.project && <div className="fp" style={{ marginTop: 6 }}>项目：{prompt.manifest.project}</div>}
      </div>
      <div className="lines">
        {s.lines.map((l) => (
          <div className="line" key={l.service + l.key_type}>
            <span className="svc">{l.service_label}</span>
            <span className="grow">{l.key_label}</span>
            <span className="tag">{l.access === "read" ? "只读" : "读写"}</span>
            {!l.available && !isRotate && <span className="tag warn">保险库中没有</span>}
          </div>
        ))}
      </div>
      {isRotate && prompt.kind.kind === "rotate" && (
        <div className="notice warn">
          agent 收到 {prompt.kind.status_code ?? "错误"}{prompt.kind.message ? `：${prompt.kind.message}` : ""}。
          批准后 PassValet 会在你的浏览器里创建新 key 并撤销旧 key。
        </div>
      )}
      {!isRotate && s.lines.some((l) => !l.available) && (
        <div className="notice warn">部分 key 尚未录入，批准后 agent 会拿到「不存在」的错误，需先在「密钥 / 自动采集」里添加。</div>
      )}
      <div className="muted" style={{ fontSize: 12 }}>
        授权时长 {s.ttl_label}{prompt.vault_locked ? " · 批准时会先解锁保险库" : ""}
      </div>
      {err && <div className="error">{err}</div>}
      <div className="actions">
        <button disabled={busy} onClick={() => decide(false)}>拒绝</button>
        <button className="primary" disabled={busy} onClick={() => decide(true)}>
          {busy ? "验证中…" : isRotate ? "批准并轮换" : "批准"}
        </button>
      </div>
    </div>
  );
}
