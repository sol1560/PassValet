import { useEffect, useState } from "react";
import { api, errorText, fmtTime, onVaultChanged } from "../lib/api";
import type { AuditEntry, AuditEvent } from "../lib/types";
import { useToast } from "../lib/toast";

const LABEL: Record<AuditEvent, string> = {
  vault_initialized: "创建保险库",
  vault_unlocked: "解锁",
  vault_locked: "锁定",
  secret_added: "添加密钥",
  secret_updated: "更新密钥",
  secret_deleted: "删除密钥",
  session_requested: "请求授权",
  session_approved: "批准授权",
  session_denied: "拒绝授权",
  session_revoked: "撤销会话",
  key_read: "读取密钥",
  key_denied: "拒绝读取",
  key_reported_invalid: "报告失效",
  rotation_started: "开始轮换",
  rotation_completed: "轮换完成",
  rotation_failed: "轮换失败",
  collection_started: "开始采集",
  collection_completed: "采集完成",
  collection_aborted: "采集中止",
  recovery_used: "使用恢复密钥",
};

function tone(ev: AuditEvent): string {
  switch (ev) {
    case "key_read":
    case "session_approved":
    case "rotation_completed":
    case "collection_completed":
    case "secret_added":
    case "vault_unlocked":
      return "ok";
    case "key_denied":
    case "session_denied":
    case "rotation_failed":
    case "collection_aborted":
    case "secret_deleted":
    case "session_revoked":
      return "danger";
    case "key_reported_invalid":
    case "recovery_used":
    case "rotation_started":
      return "warn";
    default:
      return "";
  }
}

export default function Audit() {
  const toast = useToast();
  const [entries, setEntries] = useState<AuditEntry[]>([]);
  const [filter, setFilter] = useState("");

  const load = () => api.auditLog(500).then(setEntries).catch((e) => toast(errorText(e), true));
  useEffect(() => {
    load();
    const un = onVaultChanged(load);
    return () => { un.then((u) => u()); };
  }, []); // eslint-disable-line react-hooks/exhaustive-deps

  const shown = entries.filter((e) => {
    if (!filter) return true;
    const hay = `${LABEL[e.event]} ${e.event} ${e.agent ?? ""} ${e.service ?? ""} ${e.key_type ?? ""} ${e.detail ?? ""}`.toLowerCase();
    return hay.includes(filter.toLowerCase());
  });

  return (
    <div>
      <div className="page-head">
        <div>
          <h1>访问日志</h1>
          <p className="muted" style={{ margin: 0 }}>谁、什么时候、拿了哪个 key。日志只存本机。</p>
        </div>
        <input style={{ width: 240 }} placeholder="筛选：agent / 服务 / 事件" value={filter} onChange={(e) => setFilter(e.target.value)} />
      </div>
      <div className="card">
        <table>
          <thead>
            <tr><th style={{ width: 150 }}>时间</th><th style={{ width: 110 }}>事件</th><th>Agent</th><th>密钥</th><th>详情</th></tr>
          </thead>
          <tbody>
            {shown.map((e) => (
              <tr key={e.id}>
                <td className="muted">{fmtTime(e.ts)}</td>
                <td><span className={"tag " + tone(e.event)}>{LABEL[e.event]}</span></td>
                <td>{e.agent ?? <span className="muted">—</span>}</td>
                <td>{e.service ? <code>{e.service}{e.key_type ? "/" + e.key_type : ""}</code> : <span className="muted">—</span>}</td>
                <td className="muted" style={{ wordBreak: "break-all" }}>{e.detail}</td>
              </tr>
            ))}
          </tbody>
        </table>
        {shown.length === 0 && <p className="muted">没有记录。</p>}
      </div>
    </div>
  );
}
