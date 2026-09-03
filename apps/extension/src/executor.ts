// Browser tool executor on top of chrome.debugger (CDP).

import { buildTree, findMatches, type AXNodeRaw, type RefEntry } from "./a11y";
import { redact } from "./redact";
import { rpcError } from "./rpc";

export interface ToolOutput {
  text: string;
  image_png_base64?: string;
  secret?: string;
  browser_state?: { tab_id: string; url: string; title: string };
  is_error?: boolean;
}

interface RunSession {
  runId: string;
  tabIds: number[];
  current: number;
  groupId: number | null;
  refs: Map<string, RefEntry>;
  refCounter: number;
  lastRows: ReturnType<typeof buildTree>["rows"];
  attached: Set<number>;
  aborted: boolean;
}

const CLIPBOARD_HOOK = `(() => {
  try {
    if (window.__pvHooked) return; window.__pvHooked = true; window.__pvClipboard = "";
    const cb = navigator.clipboard;
    if (cb) {
      const origWriteText = cb.writeText ? cb.writeText.bind(cb) : null;
      try { Object.defineProperty(cb, "writeText", { configurable: true, writable: true, value: (t) => { try { window.__pvClipboard = String(t); } catch {} return origWriteText ? origWriteText(t) : Promise.resolve(); } }); } catch {}
      const origWrite = cb.write ? cb.write.bind(cb) : null;
      try { Object.defineProperty(cb, "write", { configurable: true, writable: true, value: async (items) => { try { for (const it of items) { if (it.types && it.types.includes("text/plain")) { const b = await it.getType("text/plain"); window.__pvClipboard = await b.text(); } } } catch {} return origWrite ? origWrite(items) : Promise.resolve(); } }); } catch {}
    }
    document.addEventListener("copy", (e) => {
      try {
        const d = e.clipboardData && e.clipboardData.getData("text/plain");
        if (d) { window.__pvClipboard = d; return; }
        const s = document.getSelection && document.getSelection().toString();
        if (s) window.__pvClipboard = s;
      } catch {}
    }, true);
  } catch {}
})();`;

export class Executor {
  private sessions = new Map<string, RunSession>();
  public onEvent: ((ev: Record<string, unknown>) => void) | null = null;

  constructor() {
    chrome.tabs.onRemoved.addListener((tabId) => {
      for (const s of this.sessions.values()) {
        if (s.tabIds.includes(tabId)) {
          s.tabIds = s.tabIds.filter((t) => t !== tabId);
          s.attached.delete(tabId);
          this.onEvent?.({ kind: "tab_closed", run_id: s.runId, tab_id: String(tabId) });
          if (s.tabIds.length === 0) s.aborted = true;
        }
      }
    });
    chrome.debugger.onDetach.addListener((source, reason) => {
      if (source.tabId === undefined) return;
      for (const s of this.sessions.values()) {
        if (s.attached.has(source.tabId)) {
          s.attached.delete(source.tabId);
          this.onEvent?.({ kind: "debugger_detached", run_id: s.runId, tab_id: String(source.tabId), reason });
        }
      }
    });
  }

  activeRuns(): string[] {
    return [...this.sessions.keys()];
  }

  abortAll() {
    for (const s of this.sessions.values()) {
      s.aborted = true;
      this.onEvent?.({ kind: "user_aborted", run_id: s.runId });
    }
  }

  // ------------------------------------------------------------------ session

  async sessionBegin(runId: string, url: string): Promise<{ tab_id: string }> {
    const tab = await chrome.tabs.create({ url, active: true });
    if (tab.id === undefined) throw rpcError("tab_failed", "could not create tab");
    let groupId: number | null = null;
    try {
      groupId = await chrome.tabs.group({ tabIds: [tab.id] });
      await chrome.tabGroups.update(groupId, { title: "PassValet", color: "purple" });
    } catch (e) {
      console.warn("[passvalet] tab group failed", e);
    }
    const s: RunSession = {
      runId,
      tabIds: [tab.id],
      current: tab.id,
      groupId,
      refs: new Map(),
      refCounter: 0,
      lastRows: [],
      attached: new Set(),
      aborted: false,
    };
    this.sessions.set(runId, s);
    await this.attach(s, tab.id);
    await this.waitForLoad(tab.id, 20000);
    await this.installHook(tab.id);
    return { tab_id: String(tab.id) };
  }

  async sessionEnd(runId: string, keepTabs: boolean): Promise<void> {
    const s = this.sessions.get(runId);
    if (!s) return;
    for (const tabId of s.attached) {
      try {
        await this.cdp(tabId, "Runtime.evaluate", { expression: "window.__pvClipboard = ''; void 0" });
      } catch {}
      try {
        await chrome.debugger.detach({ tabId });
      } catch {}
    }
    s.attached.clear();
    if (!keepTabs) {
      for (const tabId of s.tabIds) {
        try {
          await chrome.tabs.remove(tabId);
        } catch {}
      }
    }
    s.refs.clear();
    s.lastRows = [];
    this.sessions.delete(runId);
  }

  private session(runId: string): RunSession {
    const s = this.sessions.get(runId);
    if (!s) throw rpcError("no_session", `no browser session for run ${runId}`);
    if (s.aborted) throw rpcError("aborted", "aborted by user");
    return s;
  }

  private async attach(s: RunSession, tabId: number) {
    if (s.attached.has(tabId)) return;
    await chrome.debugger.attach({ tabId }, "1.3");
    s.attached.add(tabId);
    await this.cdp(tabId, "Page.enable", {});
    await this.cdp(tabId, "DOM.enable", {});
    await this.cdp(tabId, "Accessibility.enable", {});
    await this.cdp(tabId, "Runtime.enable", {});
    await this.cdp(tabId, "Page.addScriptToEvaluateOnNewDocument", { source: CLIPBOARD_HOOK });
  }

  private async installHook(tabId: number) {
    try {
      await this.cdp(tabId, "Runtime.evaluate", { expression: CLIPBOARD_HOOK });
    } catch {}
  }

  private cdp<T = any>(tabId: number, method: string, params: Record<string, unknown>): Promise<T> {
    return new Promise((resolve, reject) => {
      chrome.debugger.sendCommand({ tabId }, method, params, (result) => {
        const err = chrome.runtime.lastError;
        if (err) reject(rpcError("cdp_error", `${method}: ${err.message}`));
        else resolve(result as T);
      });
    });
  }

  private async waitForLoad(tabId: number, timeoutMs: number) {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      const t = await chrome.tabs.get(tabId).catch(() => null);
      if (!t) return;
      if (t.status === "complete") {
        await sleep(400);
        return;
      }
      await sleep(150);
    }
  }

  private async state(tabId: number) {
    const t = await chrome.tabs.get(tabId).catch(() => null);
    return t ? { tab_id: String(tabId), url: t.url ?? "", title: t.title ?? "" } : undefined;
  }

  private tabOf(s: RunSession, params: any): number {
    if (params?.tab_id !== undefined && params.tab_id !== null && params.tab_id !== "") {
      const id = Number(params.tab_id);
      if (!s.tabIds.includes(id)) throw rpcError("bad_tab", `tab ${params.tab_id} does not belong to this run`);
      return id;
    }
    return s.current;
  }

  private resolveRef(s: RunSession, ref: string): RefEntry {
    const e = s.refs.get(ref);
    if (!e) throw rpcError("stale_ref", `unknown or stale element reference ${ref}; call read_page or find again`);
    return e;
  }

  // ------------------------------------------------------------------ dispatch

  async call(tool: string, params: any): Promise<ToolOutput> {
    const runId = params?.run_id;
    if (!runId) throw rpcError("invalid_params", "run_id missing");
    const s = this.session(runId);
    const tabId = this.tabOf(s, params);
    await this.attach(s, tabId);
    const out = await this.dispatch(s, tabId, tool, params);
    if (!out.browser_state && tool !== "screenshot") out.browser_state = await this.state(tabId);
    return out;
  }

  private async dispatch(s: RunSession, tabId: number, tool: string, p: any): Promise<ToolOutput> {
    switch (tool) {
      case "navigate":
        return this.navigate(s, tabId, String(p.url ?? ""));
      case "read_page":
        return this.readPage(s, tabId, p);
      case "find":
        return this.find(s, tabId, String(p.query ?? ""));
      case "get_page_text":
        return this.pageText(tabId);
      case "screenshot":
        return this.screenshot(tabId);
      case "left_click":
        return this.click(s, tabId, p, 1);
      case "double_click":
        return this.click(s, tabId, p, 2);
      case "hover":
        return this.hover(s, tabId, p);
      case "type":
        return this.type(tabId, String(p.text ?? ""));
      case "key":
        return this.key(tabId, String(p.text ?? ""), Number(p.repeat ?? 1));
      case "form_input":
        return this.formInput(s, tabId, p);
      case "scroll":
        return this.scroll(tabId, p);
      case "scroll_to":
        return this.scrollTo(s, tabId, String(p.ref));
      case "wait": {
        const secs = Math.min(10, Math.max(0.2, Number(p.seconds ?? 1)));
        await sleep(secs * 1000);
        return { text: `waited ${secs}s` };
      }
      case "list_tabs": {
        const lines: string[] = [];
        for (const id of s.tabIds) {
          const st = await this.state(id);
          if (st) lines.push(`${id === s.current ? "* " : "  "}tab ${id}: ${st.title} — ${st.url}`);
        }
        return { text: lines.join("\n") || "no tabs" };
      }
      case "switch_tab": {
        const id = Number(p.tab_id);
        if (!s.tabIds.includes(id)) throw rpcError("bad_tab", `tab ${p.tab_id} not in this run`);
        s.current = id;
        await chrome.tabs.update(id, { active: true });
        return { text: `switched to tab ${id}` };
      }
      case "capture_secret":
        return this.captureSecret(s, tabId, p);
      default:
        throw rpcError("unknown_tool", `unknown browser tool ${tool}`);
    }
  }

  // ------------------------------------------------------------------ tools

  private async navigate(s: RunSession, tabId: number, url: string): Promise<ToolOutput> {
    if (url === "back" || url === "forward") {
      await (url === "back" ? chrome.tabs.goBack(tabId) : chrome.tabs.goForward(tabId));
    } else if (url === "reload") {
      await chrome.tabs.reload(tabId);
    } else {
      let target = url.trim();
      if (!/^https?:\/\//i.test(target)) {
        if (/^[a-z][a-z0-9+.-]*:/i.test(target)) throw rpcError("bad_scheme", "only http(s) URLs are allowed");
        target = "https://" + target;
      }
      await chrome.tabs.update(tabId, { url: target });
    }
    await this.waitForLoad(tabId, 30000);
    await this.installHook(tabId);
    s.refs.clear();
    const st = await this.state(tabId);
    return { text: `navigated: ${st?.title ?? ""} ${st?.url ?? ""}`, browser_state: st };
  }

  private async fullTree(tabId: number, depth?: number): Promise<AXNodeRaw[]> {
    const res = await this.cdp<{ nodes: AXNodeRaw[] }>(tabId, "Accessibility.getFullAXTree", depth ? { depth } : {});
    return res.nodes ?? [];
  }

  private async readPage(s: RunSession, tabId: number, p: any): Promise<ToolOutput> {
    const nodes = await this.fullTree(tabId);
    let scope: number | undefined;
    if (p.ref) scope = this.resolveRef(s, String(p.ref)).backendNodeId;
    const built = buildTree(
      nodes,
      {
        interactiveOnly: p.filter === "interactive",
        maxDepth: Math.min(60, Math.max(1, Number(p.depth ?? 40))),
        scopeBackendNodeId: scope,
        maxChars: 50000,
      },
      s.refCounter,
    );
    // Merge refs (keep old ones valid until navigation) and advance the counter.
    for (const [k, v] of built.refs) s.refs.set(k, v);
    s.refCounter += built.refs.size;
    s.lastRows = built.rows;
    return { text: redact(built.text) };
  }

  private async find(s: RunSession, tabId: number, query: string): Promise<ToolOutput> {
    if (!query) throw rpcError("invalid_params", "query required");
    const nodes = await this.fullTree(tabId);
    const built = buildTree(nodes, { interactiveOnly: false, maxDepth: 60, maxChars: 400000 }, s.refCounter);
    for (const [k, v] of built.refs) s.refs.set(k, v);
    s.refCounter += built.refs.size;
    s.lastRows = built.rows;
    const rows = findMatches(built.rows, query);
    if (rows.length === 0) return { text: `no elements match "${query}"; try read_page filter="interactive"` };
    return { text: redact(rows.map((r) => r.line).join("\n")) };
  }

  private async pageText(tabId: number): Promise<ToolOutput> {
    const r = await this.cdp<{ result: { value?: string } }>(tabId, "Runtime.evaluate", {
      expression: "(document.body && document.body.innerText) || ''",
      returnByValue: true,
    });
    let t = String(r.result?.value ?? "");
    if (t.length > 50000) t = t.slice(0, 50000) + "\n… truncated";
    return { text: redact(t) };
  }

  private async screenshot(tabId: number): Promise<ToolOutput> {
    const metrics = await this.cdp<any>(tabId, "Page.getLayoutMetrics", {});
    const shot = await this.cdp<{ data: string }>(tabId, "Page.captureScreenshot", { format: "png", captureBeyondViewport: false });
    const vw = Math.round(metrics.cssVisualViewport?.clientWidth ?? metrics.layoutViewport?.clientWidth ?? 0);
    const vh = Math.round(metrics.cssVisualViewport?.clientHeight ?? metrics.layoutViewport?.clientHeight ?? 0);
    return {
      text: `screenshot of viewport ${vw}x${vh} CSS px (coordinates are in this space)`,
      image_png_base64: shot.data,
      browser_state: await this.state(tabId),
    };
  }

  private async centerOf(s: RunSession, tabId: number, p: any): Promise<{ x: number; y: number; desc: string }> {
    if (p.ref) {
      const e = this.resolveRef(s, String(p.ref));
      try {
        await this.cdp(tabId, "DOM.scrollIntoViewIfNeeded", { backendNodeId: e.backendNodeId });
      } catch {}
      const box = await this.cdp<{ model: { content: number[] } }>(tabId, "DOM.getBoxModel", { backendNodeId: e.backendNodeId }).catch(() => null);
      if (!box) throw rpcError("stale_ref", `${p.ref} is not rendered (hidden or gone); re-read the page`);
      const q = box.model.content;
      const x = (q[0] + q[2] + q[4] + q[6]) / 4;
      const y = (q[1] + q[3] + q[5] + q[7]) / 4;
      return { x, y, desc: `${p.ref} (${e.role} "${e.name}")` };
    }
    if (Array.isArray(p.coordinate) && p.coordinate.length === 2) {
      return { x: Number(p.coordinate[0]), y: Number(p.coordinate[1]), desc: `(${p.coordinate[0]}, ${p.coordinate[1]})` };
    }
    throw rpcError("invalid_params", "ref or coordinate required");
  }

  private async mouse(tabId: number, type: string, x: number, y: number, extra: Record<string, unknown> = {}) {
    await this.cdp(tabId, "Input.dispatchMouseEvent", { type, x, y, ...extra });
  }

  private async click(s: RunSession, tabId: number, p: any, clicks: number): Promise<ToolOutput> {
    const { x, y, desc } = await this.centerOf(s, tabId, p);
    await this.mouse(tabId, "mouseMoved", x, y);
    for (let i = 1; i <= clicks; i++) {
      await this.mouse(tabId, "mousePressed", x, y, { button: "left", clickCount: i });
      await this.mouse(tabId, "mouseReleased", x, y, { button: "left", clickCount: i });
    }
    await sleep(350);
    await this.waitForLoad(tabId, 5000);
    const st = await this.state(tabId);
    return { text: `${clicks === 2 ? "double-clicked" : "clicked"} ${desc}`, browser_state: st };
  }

  private async hover(s: RunSession, tabId: number, p: any): Promise<ToolOutput> {
    const { x, y, desc } = await this.centerOf(s, tabId, p);
    await this.mouse(tabId, "mouseMoved", x, y);
    await sleep(300);
    return { text: `hovering ${desc}` };
  }

  private async type(tabId: number, text: string): Promise<ToolOutput> {
    if (!text) return { text: "nothing to type" };
    await this.cdp(tabId, "Input.insertText", { text });
    await sleep(120);
    return { text: `typed ${text.length} characters` };
  }

  private async key(tabId: number, chord: string, repeat: number): Promise<ToolOutput> {
    const seq = chord.trim().split(/\s+/);
    for (let r = 0; r < Math.min(20, Math.max(1, repeat)); r++) {
      for (const combo of seq) {
        const parts = combo.split("+");
        const keyName = parts.pop()!;
        let modifiers = 0;
        for (const m of parts.map((x) => x.toLowerCase())) {
          if (m === "alt" || m === "option") modifiers |= 1;
          else if (m === "ctrl" || m === "control") modifiers |= 2;
          else if (m === "meta" || m === "cmd" || m === "command") modifiers |= 4;
          else if (m === "shift") modifiers |= 8;
        }
        const def = KEYS[keyName.toLowerCase()] ?? KEYS[keyName] ?? { key: keyName, code: `Key${keyName.toUpperCase()}`, keyCode: keyName.toUpperCase().charCodeAt(0) };
        const text = def.key.length === 1 && modifiers === 0 ? def.key : undefined;
        await this.cdp(tabId, "Input.dispatchKeyEvent", { type: text ? "keyDown" : "rawKeyDown", modifiers, key: def.key, code: def.code, windowsVirtualKeyCode: def.keyCode, nativeVirtualKeyCode: def.keyCode, text, unmodifiedText: text });
        await this.cdp(tabId, "Input.dispatchKeyEvent", { type: "keyUp", modifiers, key: def.key, code: def.code, windowsVirtualKeyCode: def.keyCode, nativeVirtualKeyCode: def.keyCode });
        await sleep(60);
      }
    }
    await sleep(200);
    return { text: `pressed ${chord}${repeat > 1 ? ` x${repeat}` : ""}` };
  }

  private async callOnRef(s: RunSession, tabId: number, ref: string, fnBody: string, args: unknown[] = []) {
    const e = this.resolveRef(s, ref);
    const { object } = await this.cdp<{ object: { objectId: string } }>(tabId, "DOM.resolveNode", { backendNodeId: e.backendNodeId });
    const r = await this.cdp<{ result: { value?: unknown }; exceptionDetails?: any }>(tabId, "Runtime.callFunctionOn", {
      objectId: object.objectId,
      functionDeclaration: fnBody,
      arguments: args.map((v) => ({ value: v })),
      returnByValue: true,
      awaitPromise: true,
    });
    if (r.exceptionDetails) throw rpcError("js_error", r.exceptionDetails.exception?.description ?? "script failed");
    return r.result?.value;
  }

  private async formInput(s: RunSession, tabId: number, p: any): Promise<ToolOutput> {
    const ref = String(p.ref);
    const value = p.value;
    const res = await this.callOnRef(
      s,
      tabId,
      ref,
      `function(v) {
        const el = this;
        const fire = (t) => el.dispatchEvent(new Event(t, { bubbles: true }));
        const tag = (el.tagName || "").toLowerCase();
        if (tag === "input" && (el.type === "checkbox" || el.type === "radio")) { el.focus(); if (!!el.checked !== !!v) el.click(); return "toggled to " + el.checked; }
        if (tag === "select") {
          let opt = [...el.options].find(o => o.value == v) || [...el.options].find(o => o.text.trim() == String(v).trim());
          if (!opt) return "ERROR: no option " + v;
          el.value = opt.value; fire("input"); fire("change"); return "selected " + opt.text;
        }
        if (tag === "input" || tag === "textarea") {
          el.focus();
          const proto = tag === "input" ? HTMLInputElement.prototype : HTMLTextAreaElement.prototype;
          const setter = Object.getOwnPropertyDescriptor(proto, "value").set;
          setter.call(el, String(v)); fire("input"); fire("change"); return "set value (" + String(v).length + " chars)";
        }
        if (el.isContentEditable) { el.focus(); el.textContent = String(v); fire("input"); return "set contenteditable text"; }
        const inner = el.querySelector && el.querySelector("input,textarea,select");
        if (inner) { inner.focus(); const proto = inner.tagName === "TEXTAREA" ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype; const d = Object.getOwnPropertyDescriptor(proto, "value"); if (d && d.set) { d.set.call(inner, String(v)); inner.dispatchEvent(new Event("input", { bubbles: true })); inner.dispatchEvent(new Event("change", { bubbles: true })); return "set nested input value"; } }
        return "ERROR: element is not a form control";
      }`,
      [value],
    );
    const txt = String(res);
    return { text: `${ref}: ${txt}`, is_error: txt.startsWith("ERROR") };
  }

  private async scroll(tabId: number, p: any): Promise<ToolOutput> {
    const metrics = await this.cdp<any>(tabId, "Page.getLayoutMetrics", {});
    const vw = metrics.cssVisualViewport?.clientWidth ?? 800;
    const vh = metrics.cssVisualViewport?.clientHeight ?? 600;
    const x = Array.isArray(p.coordinate) ? Number(p.coordinate[0]) : vw / 2;
    const y = Array.isArray(p.coordinate) ? Number(p.coordinate[1]) : vh / 2;
    const amount = Math.min(10, Math.max(1, Number(p.amount ?? 3))) * 100;
    const dir = String(p.direction ?? "down");
    const deltaX = dir === "left" ? -amount : dir === "right" ? amount : 0;
    const deltaY = dir === "up" ? -amount : dir === "down" ? amount : 0;
    await this.cdp(tabId, "Input.dispatchMouseEvent", { type: "mouseWheel", x, y, deltaX, deltaY });
    await sleep(300);
    return { text: `scrolled ${dir} ${amount}px` };
  }

  private async scrollTo(s: RunSession, tabId: number, ref: string): Promise<ToolOutput> {
    const e = this.resolveRef(s, ref);
    await this.cdp(tabId, "DOM.scrollIntoViewIfNeeded", { backendNodeId: e.backendNodeId });
    await sleep(200);
    return { text: `scrolled ${ref} into view` };
  }

  private async captureSecret(s: RunSession, tabId: number, p: any): Promise<ToolOutput> {
    const source = String(p.source ?? (p.ref ? "element" : "clipboard"));
    let value = "";
    if (source === "clipboard") {
      const r = await this.cdp<{ result: { value?: string } }>(tabId, "Runtime.evaluate", {
        expression: "String(window.__pvClipboard || '')",
        returnByValue: true,
      });
      value = String(r.result?.value ?? "").trim();
      if (!value) {
        // last resort: ask the page for the clipboard (needs focus + permission)
        try {
          const r2 = await this.cdp<{ result: { value?: string } }>(tabId, "Runtime.evaluate", {
            expression: "navigator.clipboard.readText().catch(() => '')",
            awaitPromise: true,
            returnByValue: true,
          });
          value = String(r2.result?.value ?? "").trim();
        } catch {}
      }
      if (!value) return { text: "clipboard is empty: click the Copy button first, then call capture_secret again with source=\"clipboard\"", is_error: true };
    } else {
      if (!p.ref) throw rpcError("invalid_params", "ref required for source=element");
      const v = await this.callOnRef(
        s,
        tabId,
        String(p.ref),
        `function() {
          const el = this;
          const tag = (el.tagName || "").toLowerCase();
          if ((tag === "input" || tag === "textarea") && el.value) return el.value;
          const inner = el.querySelector && el.querySelector("input,textarea");
          if (inner && inner.value) return inner.value;
          const t = (el.innerText || el.textContent || "").trim();
          if (t) return t;
          const title = el.getAttribute && (el.getAttribute("title") || el.getAttribute("aria-label") || el.getAttribute("data-value") || el.getAttribute("value"));
          return title || "";
        }`,
      );
      value = String(v ?? "").trim();
      // Elements often show "sk_live_… Copy"; keep the longest token-ish chunk.
      if (value && /\s/.test(value)) {
        const parts = value.split(/\s+/).filter((x) => x.length >= 8).sort((a, b) => b.length - a.length);
        if (parts.length) value = parts[0];
      }
      if (!value) return { text: "no value found at that element (empty or masked). Reveal the key first, or click Copy and use source=\"clipboard\".", is_error: true };
      if (/^[•*●]+$/.test(value) || /^\*{6,}$/.test(value)) return { text: "the element only shows masked dots; click Reveal/Show first", is_error: true };
    }
    try {
      await this.cdp(tabId, "Runtime.evaluate", { expression: "window.__pvClipboard = ''; void 0" });
    } catch {}
    return { text: "captured", secret: value };
  }
}

const KEYS: Record<string, { key: string; code: string; keyCode: number }> = {
  enter: { key: "Enter", code: "Enter", keyCode: 13 },
  return: { key: "Enter", code: "Enter", keyCode: 13 },
  tab: { key: "Tab", code: "Tab", keyCode: 9 },
  escape: { key: "Escape", code: "Escape", keyCode: 27 },
  esc: { key: "Escape", code: "Escape", keyCode: 27 },
  backspace: { key: "Backspace", code: "Backspace", keyCode: 8 },
  delete: { key: "Delete", code: "Delete", keyCode: 46 },
  space: { key: " ", code: "Space", keyCode: 32 },
  arrowup: { key: "ArrowUp", code: "ArrowUp", keyCode: 38 },
  arrowdown: { key: "ArrowDown", code: "ArrowDown", keyCode: 40 },
  arrowleft: { key: "ArrowLeft", code: "ArrowLeft", keyCode: 37 },
  arrowright: { key: "ArrowRight", code: "ArrowRight", keyCode: 39 },
  up: { key: "ArrowUp", code: "ArrowUp", keyCode: 38 },
  down: { key: "ArrowDown", code: "ArrowDown", keyCode: 40 },
  left: { key: "ArrowLeft", code: "ArrowLeft", keyCode: 37 },
  right: { key: "ArrowRight", code: "ArrowRight", keyCode: 39 },
  home: { key: "Home", code: "Home", keyCode: 36 },
  end: { key: "End", code: "End", keyCode: 35 },
  pageup: { key: "PageUp", code: "PageUp", keyCode: 33 },
  pagedown: { key: "PageDown", code: "PageDown", keyCode: 34 },
  a: { key: "a", code: "KeyA", keyCode: 65 },
  c: { key: "c", code: "KeyC", keyCode: 67 },
  v: { key: "v", code: "KeyV", keyCode: 86 },
  x: { key: "x", code: "KeyX", keyCode: 88 },
  z: { key: "z", code: "KeyZ", keyCode: 90 },
  f: { key: "f", code: "KeyF", keyCode: 70 },
};

function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}
