import { useEffect, useState } from "react";
import { api, errorText, onRunEvent } from "../lib/api";
import type { Playbook, RunEvent, RunInfo, VaultInfo } from "../lib/types";
import { useToast } from "../lib/toast";

export default function Collect({ info, extensionConnected }: { info: VaultInfo; extensionConnected: boolean }) {
  const toast = useToast();
  const [playbooks, setPlaybooks] = useState<Playbook[]>([]);
  const [runs, setRuns] = useState<RunInfo[]>([]);
  const [selected, setSelected] = useState<string>("");
  const [keyTypes, setKeyTypes] = useState<string[]>([]);
  const [hint, setHint] = useState("");

  const loadRuns = () => api.listRuns().then(setRuns).catch(() => {});
  useEffect(() => {
    api.listPlaybooks().then((p) => { setPlaybooks(p); if (!selected && p[0]) { setSelected(p[0].service); setKeyTypes(p[0].key_types); } }).catch((e) => toast(errorText(e), true));
    loadRuns();
    const un = onRunEvent(() => loadRuns());
    return () => { un.then((u) => u()); };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const pb = playbooks.find((p) => p.service === selected);
  const active = runs.find((r) => !r.finished);

  async function start() {
    try {
      await api.startCollection(selected, keyTypes, hint.trim() ? [hint.trim()] : undefined);
      setHint("");
      loadRuns();
    } catch (e) {
      toast(errorText(e), true);
    }
  }

  return (
    <div>
      <div className="page-head">
        <div>
          <h1>自动采集</h1>
          <p className="muted" style={{ margin: 0 }}>
            在你已登录的 Chrome 里打开控制台，由模型导航到 API key 页面并把 key 直接存入保险库。模型只看到脱敏后的页面，永远看不到 key 本身。
          </p>
        </div>
      </div>

      {!extensionConnected && (
        <div className="notice warn" style={{ marginBottom: 12 }}>
          浏览器扩展未连接。请在「设置 → 浏览器扩展」中完成安装；Chrome 打开且扩展启用后这里会自动变绿。
        </div>
      )}
      {info.locked && <div className="notice warn" style={{ marginBottom: 12 }}>保险库已锁定，采集前请先解锁。</div>}

      {!active && (
        <div className="card col">
          <h2>添加服务</h2>
          <div className="row wrap" style={{ gap: 8 }}>
            {playbooks.map((p) => (
              <button key={p.service} className={p.service === selected ? "primary" : ""} onClick={() => { setSelected(p.service); setKeyTypes(p.key_types); }}>
                {p.label}
              </button>
            ))}
          </div>
          {pb && (
            <>
              <div className="field" style={{ margin: 0 }}>
                <label>要采集的密钥</label>
                <div className="row wrap" style={{ gap: 8 }}>
                  {pb.key_types.map((k) => (
                    <label key={k} className="tag" style={{ padding: "5px 9px", cursor: "pointer" }}>
                      <input type="checkbox" style={{ width: "auto", marginRight: 6 }} checked={keyTypes.includes(k)}
                        onChange={(e) => setKeyTypes(e.target.checked ? [...keyTypes, k] : keyTypes.filter((x) => x !== k))} />
                      {k}
                    </label>
                  ))}
                </div>
              </div>
              <div className="field" style={{ margin: 0 }}>
                <label>提示（可选）：例如「使用 my-saas 这个项目」「用 live 模式」</label>
                <input value={hint} onChange={(e) => setHint(e.target.value)} />
              </div>
              <div className="row">
                <button className="primary" disabled={!extensionConnected || info.locked || keyTypes.length === 0} onClick={start}>
                  在浏览器中开始采集 {pb.label}
                </button>
                <span className="muted">起点：{pb.start_url}</span>
              </div>
            </>
          )}
        </div>
      )}

      {runs.map((r) => <RunCard key={r.run_id} run={r} onChanged={loadRuns} />)}
      {runs.length > 0 && runs.every((r) => r.finished) && (
        <button className="ghost small" style={{ marginTop: 10 }} onClick={() => api.clearRuns().then(loadRuns)}>清除已完成记录</button>
      )}
    </div>
  );
}

function RunCard({ run, onChanged }: { run: RunInfo; onChanged: () => void }) {
  const toast = useToast();
  const [events, setEvents] = useState<RunEvent[]>(run.events);
  useEffect(() => setEvents(run.events), [run.events]);
  useEffect(() => {
    const un = onRunEvent((ev) => { if (ev.run_id === run.run_id) setEvents((e) => [...e, ev]); });
    return () => { un.then((u) => u()); };
  }, [run.run_id]);

  const needUser = [...events].reverse().find((e) => e.type === "need_user");
  const lastStepIdx = events.map((e) => e.type).lastIndexOf("step");
  const waiting = needUser && events.indexOf(needUser) > lastStepIdx - 1 && !run.finished;
  const captured = events.filter((e) => e.type === "captured");
  const kind = run.kind.kind === "collect" ? `采集 ${run.kind.key_types.join(", ")}` : `轮换 ${run.kind.key_type}`;

  return (
    <div className="card col">
      <div className="row between">
        <div className="row">
          <strong>{run.service}</strong>
          <span className="tag">{kind}</span>
          {run.finished ? <Outcome run={run} /> : <span className="tag warn">运行中</span>}
          {captured.length > 0 && <span className="tag ok">已捕获 {captured.length}</span>}
        </div>
        {!run.finished && (
          <div className="row">
            {waiting && needUser?.type === "need_user" && (
              <button className="primary small" onClick={() => api.resumeRun(run.run_id).then(onChanged)}>我已完成，继续</button>
            )}
            <button className="danger small" onClick={() => api.abortRun(run.run_id).then(() => toast("已请求中止"))}>中止</button>
          </div>
        )}
      </div>
      {waiting && needUser?.type === "need_user" && (
        <div className="notice">需要你在浏览器里操作：{needUser.message}</div>
      )}
      <div className="runlog">
        {events.map((ev, i) => <EventRow key={i} ev={ev} />)}
      </div>
    </div>
  );
}

function Outcome({ run }: { run: RunInfo }) {
  const o = run.finished!;
  switch (o.status) {
    case "success": return <span className="tag ok">成功</span>;
    case "partial": return <span className="tag warn">部分完成 · 缺 {o.missing.join(", ")}</span>;
    case "failed": return <span className="tag danger">失败</span>;
    case "aborted": return <span className="tag">已中止</span>;
    default: { const _e: never = o; return _e; }
  }
}

function EventRow({ ev }: { ev: RunEvent }) {
  switch (ev.type) {
    case "started": return <div className="ev"><span /><span className="tool">start</span><span className="res">模型 {ev.model}</span></div>;
    case "thought": return <div className="ev thought"><span /><span>思考</span><span className="res">{ev.text}</span></div>;
    case "step": return <div className={"ev" + (ev.is_error ? " error" : "")}><span className="muted">{ev.step}</span><span className="tool">{ev.tool}</span><span className="res">{ev.args}{ev.result ? `\n→ ${ev.result}` : ""}</span></div>;
    case "captured": return <div className="ev captured"><span /><span className="tool">captured</span><span className="res">{ev.key_type} {ev.fingerprint} {ev.valid ? "✓ 格式匹配" : "⚠ 格式不匹配"}</span></div>;
    case "escalated": return <div className="ev"><span /><span className="tool">escalate</span><span className="res">{ev.from} → {ev.to}</span></div>;
    case "need_user": return <div className="ev"><span /><span className="tool">need_user</span><span className="res">{ev.message}</span></div>;
    case "finished": return <div className="ev"><span /><span className="tool">finished</span><span className="res">{JSON.stringify(ev.outcome)}</span></div>;
    default: { const _e: never = ev; return _e; }
  }
}
