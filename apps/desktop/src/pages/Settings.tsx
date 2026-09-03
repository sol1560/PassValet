import { useEffect, useState } from "react";
import { api, errorText } from "../lib/api";
import type { ProviderConfig, SettingsView, VaultInfo } from "../lib/types";
import { useToast } from "../lib/toast";

const PRESETS: { id: string; label: string; cfg: ProviderConfig; note: string }[] = [
  {
    id: "zenmux-flash",
    label: "ZenMux · Gemini 3.7 Flash → Sonnet 5",
    cfg: { kind: "openai_compat", base_url: "https://zenmux.ai/api/v1", models: ["google/gemini-3.7-flash", "anthropic/claude-sonnet-5"] },
    note: "默认。便宜且够用，连续失败自动升级到 Sonnet 5。",
  },
  {
    id: "zenmux-cheap",
    label: "ZenMux · GLM-5.3 Flash → Gemini 3.7 Flash",
    cfg: { kind: "openai_compat", base_url: "https://zenmux.ai/api/v1", models: ["z-ai/glm-5.3-flash", "google/gemini-3.7-flash", "anthropic/claude-sonnet-5"] },
    note: "最省钱：先用 $0.25/M 输出的 GLM Flash。",
  },
  {
    id: "zenmux-anthropic",
    label: "ZenMux · Anthropic 协议 (Haiku 4.5 → Sonnet 5)",
    cfg: { kind: "anthropic", base_url: "https://zenmux.ai/api/anthropic", models: ["anthropic/claude-haiku-4.5", "anthropic/claude-sonnet-5"] },
    note: "走 Anthropic Messages 协议。",
  },
  {
    id: "ollama",
    label: "本地 Ollama",
    cfg: { kind: "openai_compat", base_url: "http://127.0.0.1:11434/v1", models: ["qwen3.5-vl:9b"] },
    note: "完全离线；需要一个支持图像 + tool calling 的模型（如 Qwen3.5-VL、UI-Venus-2）。",
  },
];

export default function SettingsPage({ info, onChanged }: { info: VaultInfo; onChanged: () => void }) {
  const toast = useToast();
  const [s, setS] = useState<SettingsView | null>(null);
  const [apiKey, setApiKey] = useState("");
  const [extId, setExtId] = useState("");
  const [mcp, setMcp] = useState<{ cursor: string; claude_code: string; codex: string; cli: string } | null>(null);
  const [testing, setTesting] = useState(false);
  const [recovery, setRecovery] = useState<string | null>(null);

  const load = () => api.settingsGet().then(setS).catch((e) => toast(errorText(e), true));
  useEffect(() => { load(); api.mcpConfigSnippet().then(setMcp).catch(() => {}); }, []); // eslint-disable-line react-hooks/exhaustive-deps

  if (!s) return <p className="muted">加载中…</p>;

  const save = async (patch: Parameters<typeof api.settingsSet>[0], msg = "已保存") => {
    try {
      await api.settingsSet(patch);
      await load();
      toast(msg);
    } catch (e) {
      toast(errorText(e), true);
    }
  };

  return (
    <div>
      <div className="page-head"><div><h1>设置</h1></div></div>

      <div className="card col">
        <h2>模型（用于自动采集）</h2>
        <div className="col" style={{ gap: 6 }}>
          {PRESETS.map((p) => {
            const selected = p.cfg.base_url === s.provider.base_url && p.cfg.models[0] === s.provider.models[0];
            return (
              <button key={p.id} className={"nav-item" + (selected ? " active" : "")} style={{ border: "1px solid var(--border)" }} onClick={() => save({ provider: p.cfg })}>
                <span><strong>{p.label}</strong><br /><span className="muted" style={{ fontSize: 12 }}>{p.note}</span></span>
                {selected && <span className="tag ok">当前</span>}
              </button>
            );
          })}
        </div>
        <div className="grid2">
          <div className="field">
            <label>Base URL</label>
            <input value={s.provider.base_url} onChange={(e) => setS({ ...s, provider: { ...s.provider, base_url: e.target.value } })} onBlur={() => save({ provider: s.provider })} />
          </div>
          <div className="field">
            <label>模型阶梯（逗号分隔，失败自动升级）</label>
            <input value={s.provider.models.join(", ")} onChange={(e) => setS({ ...s, provider: { ...s.provider, models: e.target.value.split(",").map((x) => x.trim()).filter(Boolean) } })} onBlur={() => save({ provider: s.provider })} />
          </div>
        </div>
        <div className="field">
          <label>API key（存入保险库，不写入配置文件）{s.has_llm_api_key && <span className="tag ok" style={{ marginLeft: 8 }}>已设置 {s.llm_api_key_preview}</span>}</label>
          <div className="row">
            <input type="password" placeholder={s.has_llm_api_key ? "•••••••• 输入以替换" : "sk-ss-v1-…（ZenMux）"} value={apiKey} onChange={(e) => setApiKey(e.target.value)} disabled={info.locked} />
            <button className="primary" disabled={!apiKey.trim() || info.locked} onClick={() => api.setLlmApiKey(apiKey).then(() => { setApiKey(""); load(); toast("API key 已保存"); }).catch((e) => toast(errorText(e), true))}>保存</button>
            <button disabled={testing || info.locked} onClick={() => { setTesting(true); api.testProvider().then((r) => toast(`${r.ok ? "连通" : "失败"} · ${r.model} · ${r.message}`, !r.ok)).catch((e) => toast(errorText(e), true)).finally(() => setTesting(false)); }}>
              {testing ? "测试中…" : "测试连接"}
            </button>
          </div>
          {info.locked && <div className="muted" style={{ fontSize: 12 }}>解锁保险库后才能修改 API key。</div>}
        </div>
      </div>

      <div className="card col">
        <h2>浏览器扩展</h2>
        <ol className="muted" style={{ margin: 0, paddingLeft: 18, lineHeight: 1.7 }}>
          <li>Chrome 打开 <code>chrome://extensions</code>，开启「开发者模式」，「加载已解压的扩展程序」选择 <code>apps/extension/.output/chrome-mv3</code>（或安装打包版）。</li>
          <li>复制扩展卡片上的 ID（32 个字母）填到下面，点「注册」——这会写入 Chrome 的 Native Messaging host 清单，指向 <code>{s.cli_path}</code>。</li>
          <li>重新加载扩展。左侧状态变为「扩展已连接」即可。</li>
        </ol>
        <div className="row">
          <input placeholder="扩展 ID" value={extId} onChange={(e) => setExtId(e.target.value)} />
          <button className="primary" disabled={extId.trim().length !== 32} onClick={() => api.installExtensionHost(extId.trim()).then((out) => { toast(out || "已注册"); setExtId(""); load(); }).catch((e) => toast(errorText(e), true))}>注册</button>
        </div>
        {s.extension_ids.length > 0 && <div className="muted" style={{ fontSize: 12 }}>已注册：{s.extension_ids.join(", ")}</div>}
      </div>

      <div className="card col">
        <h2>接入 Agent（MCP）</h2>
        <p className="muted" style={{ margin: 0 }}>把下面的配置加到你的编辑器；agent 会用 <code>request_permissions</code> → <code>get_key</code> 拿 key。传统工作流用 <code>passvalet init</code> + <code>passvalet inject</code> 写 .env。</p>
        {mcp && (
          <div className="grid2">
            <div><h3>Cursor · ~/.cursor/mcp.json</h3><pre>{mcp.cursor}</pre></div>
            <div><h3>Claude Code</h3><pre>{mcp.claude_code}</pre><h3>Codex · ~/.codex/config.toml</h3><pre>{mcp.codex}</pre></div>
          </div>
        )}
      </div>

      <div className="card col">
        <h2>安全</h2>
        <div className="grid2">
          <div className="field">
            <label>空闲自动锁定（分钟，0 = 不锁）</label>
            <input type="number" min={0} value={s.auto_lock_minutes} onChange={(e) => setS({ ...s, auto_lock_minutes: Number(e.target.value) })} onBlur={() => save({ auto_lock_minutes: s.auto_lock_minutes })} />
          </div>
          <div className="field">
            <label>每次授权都要求 Touch ID / Passkey</label>
            <label className="row" style={{ gap: 8, padding: "8px 0" }}>
              <input type="checkbox" style={{ width: "auto" }} checked={s.presence_on_approve} onChange={(e) => save({ presence_on_approve: e.target.checked })} />
              {s.presence_on_approve ? "开启（推荐）" : "关闭：解锁期间批准无需再次验证"}
            </label>
          </div>
        </div>
        <div className="row wrap">
          <span className="muted">解锁方式：{info.provider === "passkey_prf" ? "原生 Passkey (PRF)" : "Touch ID + 钥匙串"}</span>
          <button disabled={info.locked} onClick={() => api.vaultRegenerateRecovery().then((r) => setRecovery(r.recovery_key)).catch((e) => toast(errorText(e), true))}>重新生成恢复密钥</button>
          <button disabled={info.locked} onClick={() => api.vaultRebind("touchid_keychain").then((r) => { setRecovery(r.recovery_key); onChanged(); }).catch((e) => toast(errorText(e), true))}>重新绑定 Touch ID</button>
          <button disabled={info.locked} onClick={() => api.vaultRebind("passkey_prf").then((r) => { setRecovery(r.recovery_key); onChanged(); }).catch((e) => toast(errorText(e), true))}>切换到原生 Passkey</button>
        </div>
        {recovery && (
          <div className="col">
            <div className="notice warn">新的恢复密钥（旧的已失效），只显示这一次：</div>
            <div className="recovery">{recovery}</div>
            <button className="ghost small" onClick={() => setRecovery(null)}>我已保存</button>
          </div>
        )}
        <div className="grid2">
          <div className="field">
            <label>Passkey relying party 域名</label>
            <input value={s.passkey_domain} onChange={(e) => setS({ ...s, passkey_domain: e.target.value })} onBlur={() => save({ passkey_domain: s.passkey_domain })} />
          </div>
          <div className="field">
            <label>Passkey 用户名</label>
            <input value={s.passkey_username} onChange={(e) => setS({ ...s, passkey_username: e.target.value })} onBlur={() => save({ passkey_username: s.passkey_username })} />
          </div>
        </div>
        <div className="muted" style={{ fontSize: 12 }}>
          数据目录 <code>{s.app_dir}</code> · socket <code>{s.socket_path}</code>
        </div>
      </div>
    </div>
  );
}
