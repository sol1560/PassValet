import { useEffect, useMemo, useState } from "react";
import { api, errorText, fmtTime } from "../lib/api";
import type { SecretMeta, ServiceInfo, VaultInfo } from "../lib/types";
import { useToast } from "../lib/toast";

const SOURCE_LABEL: Record<SecretMeta["source"], string> = {
  manual: "手动",
  collected: "自动采集",
  rotated: "已轮换",
  imported: "导入",
};

export default function Keys({ info, onChanged }: { info: VaultInfo; onChanged: () => void }) {
  const toast = useToast();
  const [secrets, setSecrets] = useState<SecretMeta[]>([]);
  const [services, setServices] = useState<ServiceInfo[]>([]);
  const [adding, setAdding] = useState(false);
  const [revealed, setRevealed] = useState<Record<string, string>>({});
  const [recoveryMode, setRecoveryMode] = useState(false);
  const [recoveryKey, setRecoveryKey] = useState("");
  const [search, setSearch] = useState("");
  const [menu, setMenu] = useState<string | null>(null);
  const [deleting, setDeleting] = useState<SecretMeta | null>(null);
  const [deleteBusy, setDeleteBusy] = useState(false);

  const load = () => {
    api.listSecrets().then(setSecrets).catch((e) => toast(errorText(e), true));
    api.listServices().then(setServices).catch(() => {});
  };
  useEffect(load, [info.secret_count, info.locked]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => { if (info.locked) { setRevealed({}); setAdding(false); } }, [info.locked]);

  const grouped = useMemo(() => {
    const m = new Map<string, SecretMeta[]>();
    for (const s of secrets) {
      if (s.service === "passvalet") continue;
      const svc = services.find((v) => v.id === s.service);
      if (!`${s.service} ${svc?.label ?? ""} ${s.key_type} ${svc?.key_types.find((k) => k.id === s.key_type)?.label ?? ""} ${s.label ?? ""}`.toLocaleLowerCase().includes(search.trim().toLocaleLowerCase())) continue;
      m.set(s.service, [...(m.get(s.service) ?? []), s]);
    }
    return [...m.entries()];
  }, [secrets, services, search]);

  const labelOf = (service: string) => services.find((s) => s.id === service)?.label ?? service;
  const keyLabel = (service: string, kt: string) =>
    services.find((s) => s.id === service)?.key_types.find((k) => k.id === kt)?.label ?? kt;

  async function reveal(s: SecretMeta) {
    if (revealed[s.id]) {
      setRevealed((r) => { const c = { ...r }; delete c[s.id]; return c; });
      return;
    }
    try {
      const v = await api.revealSecret(s.service, s.key_type);
      setRevealed((r) => ({ ...r, [s.id]: v }));
      setTimeout(() => setRevealed((r) => { const c = { ...r }; delete c[s.id]; return c; }), 20000);
    } catch (e) {
      toast(errorText(e), true);
    }
  }

  async function copy(s: SecretMeta) {
    try {
      const v = await api.revealSecret(s.service, s.key_type);
      await navigator.clipboard.writeText(v);
      toast("已复制到剪贴板，用完后请清除。");
    } catch (e) {
      toast(errorText(e), true);
    }
  }

  async function remove(s: SecretMeta) {
    setDeleteBusy(true);
    try {
      await api.deleteSecret(s.id);
      setDeleting(null);
      onChanged();
      load();
    } catch (e) {
      toast(errorText(e), true);
    } finally {
      setDeleteBusy(false);
    }
  }

  if (info.locked) {
    return (
      <div className="center">
        <div className="onboard col card">
          <h2>保险库已锁定</h2>
          <p className="muted">解锁后才能查看或添加密钥。密钥元数据（服务名、类型、指纹）在锁定状态下也可见于会话与日志页。</p>
          <button className="primary" onClick={() => api.vaultUnlock().then(onChanged).catch((e) => toast(errorText(e), true))}>
            {info.provider === "passkey_prf" ? "用 Passkey 解锁" : "用 Touch ID 解锁"}
          </button>
          {!recoveryMode ? (
            <button className="ghost small" onClick={() => setRecoveryMode(true)}>
              使用恢复密钥
            </button>
          ) : (
            <div className="col">
              <input placeholder="XXXXX-XXXXX-…-CCCC" value={recoveryKey} onChange={(e) => setRecoveryKey(e.target.value)} />
              <div className="row">
                <button
                  className="primary"
                  onClick={() =>
                    api.vaultUnlockRecovery(recoveryKey).then(() => { setRecoveryMode(false); onChanged(); toast("已用恢复密钥解锁。建议到设置里重新绑定 Touch ID / Passkey。"); }).catch((e) => toast(errorText(e), true))
                  }
                >
                  解锁
                </button>
                <button className="ghost" onClick={() => setRecoveryMode(false)}>取消</button>
              </div>
            </div>
          )}
        </div>
      </div>
    );
  }

  return (
    <div>
      {deleting && <dialog className="delete-dialog" aria-labelledby="delete-title" ref={(node) => { if (node && !node.open) node.showModal(); }} onCancel={() => setDeleting(null)}>
        <h2 id="delete-title">删除这个密钥？</h2>
        <p>{labelOf(deleting.service)} · {keyLabel(deleting.service, deleting.key_type)}</p>
        <p className="muted">此操作不可恢复，依赖它的 Agent 将无法再读取。</p>
        <div className="actions"><button autoFocus disabled={deleteBusy} onClick={() => setDeleting(null)}>取消</button><button className="danger" disabled={deleteBusy} onClick={() => remove(deleting)}>{deleteBusy ? "删除中…" : "确认删除"}</button></div>
      </dialog>}
      <div className="page-head">
        <div>
          <h1>密钥</h1>
          <p className="muted" style={{ margin: 0 }}>{secrets.filter((s) => s.service !== "passvalet").length} 个密钥，全部加密存储在本机。</p>
        </div>
        <div className="row">
          <button className="primary" onClick={() => setAdding(true)}>＋ 添加密钥</button>
        </div>
      </div>

      {adding && <AddForm services={services} onClose={() => setAdding(false)} onAdded={() => { setAdding(false); onChanged(); load(); }} />}
      <input className="key-search" aria-label="搜索服务或密钥" placeholder="搜索服务、密钥类型或备注" value={search} onChange={(e) => setSearch(e.target.value)} />

      {grouped.length === 0 && !adding && (
        <div className="card col">
          <h2>{search.trim() ? "没有找到匹配的密钥" : "还没有密钥"}</h2>
          <p className="muted">{search.trim() ? "试试其他服务名、类型或备注。" : "添加已有密钥，或前往「自动采集」从浏览器中的服务控制台采集。"}</p>
        </div>
      )}

      <div className="service-grid">{grouped.map(([service, items]) => (
        <div className="card service-card" key={service}>
          <div className="service-head">
            <span className="service-icon" aria-hidden="true">{service === "supabase" ? "ϟ" : service === "vercel" ? "▲" : labelOf(service).slice(0, 2)}</span>
            <span className="name">{labelOf(service)}</span>
            <span className="tag">{items.length} 个密钥</span>
          </div>
          <div className="list">
            {items.map((s) => (
              <div className="list-item" key={s.id}>
                <div className="grow">
                  <div className="row wrap">
                    <strong>{keyLabel(s.service, s.key_type)}</strong>
                    <span className="tag">{s.key_type}</span>
                    <span className="tag">{SOURCE_LABEL[s.source]}</span>
                    {s.label && <span className="muted">{s.label}</span>}
                  </div>
                  <div className="fp" style={{ marginTop: 3 }}>
                    {revealed[s.id] ? revealed[s.id] : "••••••••••••"}
                    <span className="updated">更新于 {fmtTime(s.updated_at)}</span>
                    {s.last_rotated_at && ` · 轮换于 ${fmtTime(s.last_rotated_at)}`}
                  </div>
                </div>
                <div className="key-actions">
                  <button className="small" onClick={() => reveal(s)}>{revealed[s.id] ? "隐藏" : "显示"}</button>
                  <button className="small copy" onClick={() => copy(s)}>复制</button>
                  <div className="more-wrap"><button className="small ghost" aria-label={`${labelOf(service)} ${keyLabel(service, s.key_type)} 更多操作`} aria-expanded={menu === s.id} onClick={() => setMenu(menu === s.id ? null : s.id)}>•••</button>
                    {menu === s.id && <div className="more-menu"><button className="danger small" onClick={() => { setMenu(null); setDeleting(s); }}>删除密钥</button><button className="ghost small" onClick={() => setMenu(null)}>关闭菜单</button></div>}
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      ))}</div>
    </div>
  );
}

function AddForm({ services, onClose, onAdded }: { services: ServiceInfo[]; onClose: () => void; onAdded: () => void }) {
  const toast = useToast();
  const [service, setService] = useState(services[0]?.id ?? "");
  const [custom, setCustom] = useState(false);
  const [customService, setCustomService] = useState("");
  const [keyType, setKeyType] = useState("");
  const [value, setValue] = useState("");
  const [label, setLabel] = useState("");
  const svc = services.find((s) => s.id === service);
  const effService = custom ? customService : service;
  const effKeyType = custom || !svc ? keyType : keyType || svc.key_types[0]?.id || "";

  useEffect(() => {
    if (svc && !custom) setKeyType(svc.key_types[0]?.id ?? "");
  }, [service, custom, svc]);

  async function submit() {
    try {
      await api.addSecret({ service: effService, key_type: effKeyType, value, label: label || undefined });
      toast("已保存");
      onAdded();
    } catch (e) {
      toast(errorText(e), true);
    }
  }

  return (
    <div className="card col">
      <div className="row between">
        <h2 style={{ margin: 0 }}>手动添加密钥</h2>
        <button className="ghost small" onClick={onClose}>关闭</button>
      </div>
      <div className="grid2">
        <div className="field">
          <label>服务</label>
          {!custom ? (
            <select value={service} onChange={(e) => setService(e.target.value)}>
              {services.map((s) => <option key={s.id} value={s.id}>{s.label}</option>)}
            </select>
          ) : (
            <input placeholder="例如 resend、posthog" value={customService} onChange={(e) => setCustomService(e.target.value)} />
          )}
          <button className="ghost small" onClick={() => setCustom(!custom)}>{custom ? "从列表选择" : "自定义服务"}</button>
        </div>
        <div className="field">
          <label>密钥类型</label>
          {!custom && svc ? (
            <select value={effKeyType} onChange={(e) => setKeyType(e.target.value)}>
              {svc.key_types.map((k) => <option key={k.id} value={k.id}>{k.label} ({k.env_var})</option>)}
            </select>
          ) : (
            <input placeholder="api_key" value={keyType} onChange={(e) => setKeyType(e.target.value)} />
          )}
        </div>
      </div>
      <div className="field">
        <label>值</label>
        <textarea value={value} onChange={(e) => setValue(e.target.value)} placeholder="粘贴 key" />
      </div>
      <div className="field">
        <label>备注（可选）</label>
        <input value={label} onChange={(e) => setLabel(e.target.value)} placeholder="例如 生产环境 / 个人账号" />
      </div>
      <div className="row">
        <button className="primary" disabled={!effService || !effKeyType || !value.trim()} onClick={submit}>保存</button>
        <button className="ghost" onClick={onClose}>取消</button>
      </div>
    </div>
  );
}
