import { Executor } from "../src/executor";
import { NativeRpc, rpcError } from "../src/rpc";

const HOST = "ai.passvalet.host";

export default defineBackground(() => {
  const executor = new Executor();

  const rpc = new NativeRpc(HOST, async (method, params) => {
    if (method === "browser.ping") return { ok: true, runs: executor.activeRuns() };
    if (method === "browser.session_begin") return executor.sessionBegin(String(params.run_id), String(params.url));
    if (method === "browser.session_end") {
      await executor.sessionEnd(String(params.run_id), Boolean(params.keep_tabs));
      return { ok: true };
    }
    if (method.startsWith("browser.")) return executor.call(method.slice("browser.".length), params);
    throw rpcError("method_not_found", `unknown method ${method}`);
  });

  executor.onEvent = (ev) => rpc.notify("ext.event", ev);

  rpc.onStateChange = (connected) => {
    void chrome.storage.session.set({ connected });
    void chrome.action.setBadgeText({ text: connected ? "" : "!" });
    void chrome.action.setBadgeBackgroundColor({ color: "#ff5c6c" });
    if (connected) {
      rpc
        .call("ext.hello", { extension_version: chrome.runtime.getManifest().version, browser: navigator.userAgent.includes("Edg/") ? "edge" : "chrome" })
        .catch((e) => console.warn("[passvalet] hello failed", e));
    }
  };

  rpc.connect();

  // Keep the service worker alive while connected and let the app know we are here.
  chrome.alarms.create("pv-keepalive", { periodInMinutes: 0.4 });
  chrome.alarms.onAlarm.addListener((a) => {
    if (a.name !== "pv-keepalive") return;
    if (!rpc.connected) rpc.connect();
    else rpc.notify("ext.event", { kind: "log", run_id: null, level: "debug", message: "keepalive" });
  });

  chrome.runtime.onMessage.addListener((msg, _sender, sendResponse) => {
    if (msg?.type === "pv:status") {
      sendResponse({ connected: rpc.connected, runs: executor.activeRuns() });
      return true;
    }
    if (msg?.type === "pv:abort") {
      executor.abortAll();
      sendResponse({ ok: true });
      return true;
    }
    if (msg?.type === "pv:reconnect") {
      rpc.connect();
      sendResponse({ ok: true });
      return true;
    }
    return false;
  });
});
