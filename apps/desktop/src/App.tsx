import { useCallback, useEffect, useRef, useState } from "react";
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
import Home from "./pages/Home";

type Page = "home" | "keys" | "collect" | "sessions" | "audit" | "settings" | "prompts";

const NAV: { id: Page; label: string }[] = [
  { id: "home", label: "主页" },
  { id: "keys", label: "密钥" },
  { id: "collect", label: "自动采集" },
  { id: "sessions", label: "会话" },
  { id: "audit", label: "访问日志" },
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
  const [setupPending, setSetupPending] = useState<boolean | null>(null);
  const [page, setPage] = useState<Page>("home");
  const [prompts, setPrompts] = useState<PendingPrompt[]>([]);
  const [ext, setExt] = useState(false);
  const [activeRun, setActiveRun] = useState(false);
  const mainRef = useRef<HTMLElement>(null);
  useEffect(() => { mainRef.current?.scrollTo(0, 0); }, [page]);

  const refresh = useCallback(async () => {
    try {
      const next = await api.vaultInfo();
      setInfo(next);
      // 创建成功的事件不能跳过恢复密钥的保存确认。
      setSetupPending((pending) => pending ?? !next.initialized);
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

  if (!info.initialized || setupPending) {
    return (
      <>
        <div className="titlebar-drag" />
        <Setup onDone={() => { setSetupPending(false); void refresh(); }} />
      </>
    );
  }

  const content = (() => {
    switch (page) {
      case "home":
        return <Home info={info} />;
      case "keys":
        return <Keys key={String(info.locked)} info={info} onChanged={refresh} />;
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
          <div className="logo">⚿</div> PassValet
        </div>
        {prompts.length > 0 && (
          <button className={"nav-item" + (page === "prompts" ? " active" : "")} onClick={() => setPage("prompts")}>
            待确认请求 <span className="badge">{prompts.length}</span>
          </button>
        )}
        {NAV.map((n) => (
          <button key={n.id} className={"nav-item" + (page === n.id ? " active" : "")} onClick={() => setPage(n.id)}>
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={({ home: "M3 10 12 3l9 7v10H3Z M9 20v-7h6v7", keys: "M6 10V7a6 6 0 0 1 12 0v3 M4 10h16v11H4Z M12 14v3", collect: "m13 2-9 12h7l-1 8 10-13h-8Z", sessions: "M21 11a9 9 0 0 1-9 9H3l2-5a9 9 0 1 1 16-4Z", audit: "M5 3h14v18H5Z M9 8h6 M9 12h6 M9 16h4", settings: "", prompts: "" })[n.id]} /></svg>
            {n.label}
            {n.id === "collect" && activeRun && <span className="dot warn" />}
          </button>
        ))}
        <div className="spacer" />
        <button className={"nav-item" + (page === "settings" ? " active" : "")} onClick={() => setPage("settings")}>⚙　设置</button>
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
            <button className="small" onClick={() => api.vaultLock().then(refresh).catch((e) => toast(errorText(e), true))}>
              锁定
            </button>
          )}
        </div>
      </aside>
      <main className="main" ref={mainRef}>
        {prompts.length > 0 && page !== "prompts" && <button className="pending-banner" onClick={() => setPage("prompts")}>●　{prompts.length} 个待确认请求 <span>查看请求 →</span></button>}
        {prompts.length === 0 && page === "home" && <div className="pending-clear"><span className="dot ok" /> 没有待确认请求</div>}
        {content}
      </main>
    </div>
  );
}
