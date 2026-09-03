import { useEffect, useState } from "react";
import { api, errorText, fmtTime, onVaultChanged, relTime } from "../lib/api";
import type { Session } from "../lib/types";
import { useToast } from "../lib/toast";

export default function Sessions() {
  const toast = useToast();
  const [sessions, setSessions] = useState<Session[]>([]);
  const [showAll, setShowAll] = useState(false);

  const load = () => api.listSessions(showAll).then(setSessions).catch((e) => toast(errorText(e), true));
  useEffect(() => {
    load();
    const un = onVaultChanged(load);
    return () => { un.then((u) => u()); };
  }, [showAll]); // eslint-disable-line react-hooks/exhaustive-deps

  const now = Date.now();

  return (
    <div>
      <div className="page-head">
        <div>
          <h1>会话</h1>
          <p className="muted" style={{ margin: 0 }}>每次 agent 获得授权都会生成一个带过期时间的 session token。撤销后 agent 必须重新申请。</p>
        </div>
        <div className="row">
          <label className="row muted" style={{ gap: 6 }}>
            <input type="checkbox" style={{ width: "auto" }} checked={showAll} onChange={(e) => setShowAll(e.target.checked)} /> 显示已过期/已撤销
          </label>
          <button className="danger" onClick={() => api.revokeAllSessions().then((n) => { toast(`已撤销 ${n} 个会话`); load(); })}>
            撤销全部
          </button>
        </div>
      </div>
      <div className="card">
        {sessions.length === 0 && <p className="muted">没有会话。</p>}
        <div className="list">
          {sessions.map((s) => {
            const active = !s.revoked && new Date(s.expires_at).getTime() > now;
            return (
              <div className="list-item" key={s.id}>
                <div className="grow">
                  <div className="row">
                    <strong>{s.agent.name}</strong>
                    {s.agent.client && <span className="tag">{s.agent.client}</span>}
                    <span className={"tag " + (active ? "ok" : s.revoked ? "danger" : "")}>
                      {s.revoked ? "已撤销" : active ? `有效 · ${relTime(s.expires_at)}过期` : "已过期"}
                    </span>
                    <span className="muted">读取 {s.read_count} 次</span>
                  </div>
                  <div className="muted" style={{ marginTop: 3 }}>{s.purpose}</div>
                  <div className="row wrap" style={{ marginTop: 6, gap: 6 }}>
                    {s.grants.map((g) => (
                      <span className="tag accent" key={g.service + g.key_type}>
                        {g.service}/{g.key_type} · {g.access === "read" ? "只读" : "读写"}
                      </span>
                    ))}
                  </div>
                  <div className="fp" style={{ marginTop: 4 }}>
                    创建 {fmtTime(s.created_at)}{s.project && ` · ${s.project}`}
                  </div>
                </div>
                {active && (
                  <button className="small danger" onClick={() => api.revokeSession(s.id).then(load)}>撤销</button>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </div>
  );
}
