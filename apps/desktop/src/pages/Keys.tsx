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

  const load = () => {
    api.listSecrets().then(setSecrets).catch((e) => toast(errorText(e), true));
    api.listServices().then(setServices).catch(() => {});
  };
  useEffect(load, [info.secret_count, info.locked]); // eslint-disable-line react-hooks/exhaustive-deps

  const grouped = useMemo(() => {
    const m = new Map<string, SecretMeta[]>();
    for (const s of secrets) {
      if (s.service === "passvalet") continue;
      m.set(s.service, [...(m.get(s.service) ?? []), s]);
    }
    return [...m.entries()];
  }, [secrets]);

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
      toast("已复制到剪贴板（20 秒后请留意清除）");
    } catch (e) {
      toast(errorText(e), true);
    }
  }

  async function remove(s: SecretMeta) {
    if (!confirm(`删除 ${labelOf(s.service)} / ${keyLabel(s.service, s.key_type)}？此操作不可恢复。`)) return;
    try {
      await api.deleteSecret(s.id);
      onChanged();
      load();
    } catch (e) {
      toast(errorText(e), true);
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
      <div className="page-head">
        <div>
          <h1>密钥</h1>
          <p className="muted" style={{ margin: 0 }}>{secrets.filter((s) => s.service !== "passvalet").length} 个密钥，全部加密存储在本机。</p>
        </div>
        <div className="row">
          <button onClick={() => setAdding(true)}>手动添加</button>
        </div>
      </div>

      {adding && <AddForm services={services} onClose={() => setAdding(false)} onAdded={() => { setAdding(false); onChanged(); load(); }} />}

      {grouped.length === 0 && !adding && (
        <div className="card col">
          <h2>还没有密钥</h2>
          <p className="muted">两种方式：手动粘贴已有的 key，或去「自动采集」让 PassValet 在你的浏览器里打开 Supabase / Stripe / OpenAI 等控制台把 key 抓回来。</p>
        </div>
      )}

      {grouped.map(([service, items]) => (
        <div className="card" key={service}>
          <div className="service-head">
            <span className="name">{labelOf(service)}</span>
            <span className="tag">{service}</span>
          </div>
          <div className="list">
            {items.map((s) => (
              <div className="list-item" key={s.id}>
                <div className="grow">
                  <div className="row">
                    <strong>{keyLabel(s.service, s.key_type)}</strong>
                    <span className="tag">{s.key_type}</span>
                    <span className="tag">{SOURCE_LABEL[s.source]}</span>
                    {s.label && <span className="muted">{s.label}</span>}
                  </div>
                  <div className="fp" style={{ marginTop: 3 }}>
                    {revealed[s.id] ? revealed[s.id] : s.fingerprint} · 更新于 {fmtTime(s.updated_at)}
                    {s.last_rotated_at && ` · 轮换于 ${fmtTime(s.last_rotated_at)}`}
                  </div>
                </div>
                <button className="small" onClick={() => reveal(s)}>{revealed[s.id] ? "隐藏" : "显示"}</button>
                <button className="small" onClick={() => copy(s)}>复制</button>
                <button className="small danger" onClick={() => remove(s)}>删除</button>
              </div>
            ))}
          </div>
        </div>
      ))}
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
