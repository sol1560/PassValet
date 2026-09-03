import { useCallback, useEffect, useState } from "react";
import { api, errorText, onExtensionChanged, onPromptChanged, onRunEvent, onVaultChanged } from "./lib/api";
import type { PendingPrompt, VaultInfo } from "./lib/types";
import { ToastProvider, useToast } from "./lib/toast";
import Setup from "./pages/Setup";
import Keys from "./pages/Keys";
import Sessions from "./pages/Sessions";
import Audit from "./pages/Audit";
import Collect from "./pages/Collect";
import SettingsPage from "./pages/Settings";
import Prompts from "./pages/Prompts";

type Page = "keys" | "collect" | "sessions" | "audit" | "settings" | "prompts";

const NAV: { id: Page; label: string }[] = [
  { id: "keys", label: "密钥" },
  { id: "collect", label: "自动采集" },
  { id: "sessions", label: "会话" },
  { id: "audit", label: "访问日志" },
  { id: "settings", label: "设置" },
];

export default function App() {
  return (
    <ToastProvider>
      <Shell />
    </ToastProvider>
  );
}

function Shell() {
  const toast = useToast();
  const [info, setInfo] = useState<VaultInfo | null>(null);
  const [page, setPage] = useState<Page>("keys");
  const [prompts, setPrompts] = useState<PendingPrompt[]>([]);
  const [ext, setExt] = useState(false);
  const [activeRun, setActiveRun] = useState(false);

  const refresh = useCallback(async () => {
    try {
      setInfo(await api.vaultInfo());
      setPrompts(await api.promptList());
      setExt((await api.extensionStatus()).connected);
      setActiveRun((await api.listRuns()).some((r) => !r.finished));
    } catch (e) {
      toast(errorText(e), true);
    }
  }, [toast]);

  useEffect(() => {
    refresh();
    const unsubs = [
      onVaultChanged(refresh),
      onPromptChanged(refresh),
      onExtensionChanged((c) => setExt(c)),
      onRunEvent((ev) => {
        if (ev.type === "started") setActiveRun(true);
        if (ev.type === "finished") setActiveRun(false);
      }),
    ];
    return () => {
      unsubs.forEach((p) => p.then((u) => u()));
    };
  }, [refresh]);

  if (!info) return <div className="center muted">加载中…</div>;

  if (!info.initialized) {
    return (
      <>
        <div className="titlebar-drag" />
        <Setup onDone={refresh} />
      </>
    );
  }

  const content = (() => {
    switch (page) {
      case "keys":
        return <Keys info={info} onChanged={refresh} />;
      case "collect":
        return <Collect info={info} extensionConnected={ext} />;
      case "sessions":
        return <Sessions />;
      case "audit":
        return <Audit />;
      case "settings":
        return <SettingsPage info={info} onChanged={refresh} />;
      case "prompts":
        return <Prompts prompts={prompts} onChanged={refresh} />;
      default: {
        const _exhaustive: never = page;
        return _exhaustive;
      }
    }
  })();

  return (
    <div className="app">
      <div className="titlebar-drag" />
      <aside className="sidebar">
        <div className="brand">
          <div className="logo" /> PassValet
        </div>
        {prompts.length > 0 && (
          <button className={"nav-item" + (page === "prompts" ? " active" : "")} onClick={() => setPage("prompts")}>
            待确认请求 <span className="badge">{prompts.length}</span>
          </button>
        )}
        {NAV.map((n) => (
          <button key={n.id} className={"nav-item" + (page === n.id ? " active" : "")} onClick={() => setPage(n.id)}>
            {n.label}
            {n.id === "collect" && activeRun && <span className="dot warn" />}
          </button>
        ))}
        <div className="spacer" />
        <div className="lockbar">
          <span className={"dot " + (ext ? "ok" : "")} />
          <span className="grow muted">{ext ? "扩展已连接" : "扩展未连接"}</span>
        </div>
        <div className="lockbar">
          <span className={"dot " + (info.locked ? "warn" : "ok")} />
          <span className="grow">{info.locked ? "已锁定" : "已解锁"}</span>
          {info.locked ? (
            <button
              className="small primary"
              onClick={() => api.vaultUnlock().then(refresh).catch((e) => toast(errorText(e), true))}
            >
              解锁
            </button>
          ) : (
            <button className="small" onClick={() => api.vaultLock().then(refresh)}>
              锁定
            </button>
          )}
        </div>
      </aside>
      <main className="main">{content}</main>
    </div>
  );
}
