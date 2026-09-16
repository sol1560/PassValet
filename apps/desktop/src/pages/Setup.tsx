import { useEffect, useState } from "react";
import { api, errorText } from "../lib/api";
import type { UnlockProviderKind } from "../lib/types";

export default function Setup({ onDone }: { onDone: () => void }) {
  const [provider, setProvider] = useState<UnlockProviderKind>("touchid_keychain");
  const [bio, setBio] = useState(true);
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);
  const [recovery, setRecovery] = useState<string | null>(null);
  const [confirmed, setConfirmed] = useState(false);
  const [domain, setDomain] = useState("");

  useEffect(() => {
    api.biometricsAvailable().then(setBio).catch(() => setBio(false));
    api.settingsGet().then((s) => setDomain(s.passkey_domain)).catch(() => {});
  }, []);

  async function create() {
    setBusy(true);
    setErr(null);
    try {
      if (provider === "passkey_prf" && domain) {
        await api.settingsSet({ passkey_domain: domain });
      }
      const r = await api.vaultSetup(provider);
      setRecovery(r.recovery_key);
    } catch (e) {
      setErr(errorText(e));
    } finally {
      setBusy(false);
    }
  }

  if (recovery) {
    return (
      <div className="center">
        <div className="onboard col">
          <h1>保存恢复密钥</h1>
          <p className="muted">
            这是唯一一次显示。若 passkey 丢失或钥匙串被清除，只有它能打开保险库。请抄写到纸上或离线密码管理器。
          </p>
          <div className="recovery">{recovery}</div>
          <button className="ghost small" onClick={() => navigator.clipboard.writeText(recovery)}>
            复制到剪贴板
          </button>
          <label className="row" style={{ gap: 8 }}>
            <input type="checkbox" style={{ width: "auto" }} checked={confirmed} onChange={(e) => setConfirmed(e.target.checked)} />
            我已妥善保存恢复密钥
          </label>
          <button className="primary" disabled={!confirmed} onClick={onDone}>
            进入 PassValet
          </button>
        </div>
      </div>
    );
  }

  return (
    <div className="center">
      <div className="onboard col">
        <div className="row" style={{ gap: 10 }}>
          <div className="logo" aria-hidden="true">⚿</div>
          <div>
            <h1>欢迎使用 PassValet</h1>
            <p className="muted" style={{ margin: 0 }}>给 vibe coder 和 AI agent 的密钥管家。密钥只存在这台设备上，加密后落盘。</p>
          </div>
        </div>

        <h3>选择保险库的解锁方式</h3>
        <div className="choice">
          <button className={provider === "touchid_keychain" ? "selected" : ""} onClick={() => setProvider("touchid_keychain")}>
            <strong>Touch ID + 钥匙串</strong>
            <span>主密钥由 macOS 钥匙串保护，每次授权按一次 Touch ID。{!bio && " （未检测到 Touch ID，会退回到账户密码）"}</span>
          </button>
          <button className={provider === "passkey_prf" ? "selected" : ""} onClick={() => setProvider("passkey_prf")}>
            <strong>原生 Passkey (PRF)</strong>
            <span>加密密钥由 passkey 的 PRF 输出派生，可随 iCloud 钥匙串同步。需要 macOS 15+ 且 app 已与域名关联。</span>
          </button>
        </div>

        {provider === "passkey_prf" && (
          <div className="notice warn col" style={{ gap: 6 }}>
            <div>
              原生 passkey 需要签名的 app 带 <code>associated-domains</code> entitlement，且域名托管{" "}
              <code>/.well-known/apple-app-site-association</code>。未配置时会报「not associated with domain」。详见 docs/passkey-setup.md。
            </div>
            <div className="field" style={{ margin: 0 }}>
              <label>Relying party 域名</label>
              <input value={domain} onChange={(e) => setDomain(e.target.value)} placeholder="passvalet.app" />
            </div>
          </div>
        )}

        {err && <div className="error">{err}</div>}
        <button className="primary" disabled={busy} onClick={create}>
          {busy ? "创建中…" : "创建保险库"}
        </button>
        <p className="muted" style={{ fontSize: 12 }}>
          已有保险库但换了设备？v1 不做云同步；把 <code>~/Library/Application Support/PassValet/vault.sqlite</code> 拷过来并用恢复密钥解锁。
        </p>
      </div>
    </div>
  );
}
