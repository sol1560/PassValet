// JSON-RPC over the native-messaging port. Chrome frames messages for us.

export interface RpcError {
  code: number;
  message: string;
  data?: unknown;
}

type Handler = (method: string, params: any) => Promise<unknown>;

export class NativeRpc {
  private port: chrome.runtime.Port | null = null;
  private nextId = 1;
  private pending = new Map<number, { resolve: (v: any) => void; reject: (e: RpcError) => void }>();
  private handler: Handler;
  private reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  private backoff = 1000;
  public connected = false;
  public onStateChange: ((connected: boolean) => void) | null = null;

  constructor(private hostName: string, handler: Handler) {
    this.handler = handler;
  }

  connect() {
    if (this.port) return;
    try {
      const port = chrome.runtime.connectNative(this.hostName);
      this.port = port;
      port.onMessage.addListener((msg) => this.onMessage(msg));
      port.onDisconnect.addListener(() => {
        const err = chrome.runtime.lastError?.message;
        console.warn("[passvalet] native host disconnected", err ?? "");
        this.port = null;
        this.setConnected(false);
        for (const [, p] of this.pending) p.reject({ code: -32003, message: "disconnected" });
        this.pending.clear();
        this.scheduleReconnect();
      });
      this.setConnected(true);
      this.backoff = 1000;
    } catch (e) {
      console.warn("[passvalet] connectNative failed", e);
      this.port = null;
      this.setConnected(false);
      this.scheduleReconnect();
    }
  }

  private setConnected(v: boolean) {
    if (this.connected !== v) {
      this.connected = v;
      this.onStateChange?.(v);
    }
  }

  private scheduleReconnect() {
    if (this.reconnectTimer) return;
    this.reconnectTimer = setTimeout(() => {
      this.reconnectTimer = null;
      this.backoff = Math.min(this.backoff * 2, 30000);
      this.connect();
    }, this.backoff);
  }

  private async onMessage(msg: any) {
    if (!msg || typeof msg !== "object") return;
    if (typeof msg.method === "string") {
      const id = msg.id;
      try {
        const result = await this.handler(msg.method, msg.params ?? {});
        if (id !== undefined && id !== null) this.send({ jsonrpc: "2.0", id, result: result ?? null });
      } catch (e: any) {
        const error: RpcError =
          e && typeof e === "object" && "code" in e
            ? e
            : { code: -32000, message: String(e?.message ?? e), data: e?.data };
        if (id !== undefined && id !== null) this.send({ jsonrpc: "2.0", id, error });
      }
      return;
    }
    if (typeof msg.id === "number" && this.pending.has(msg.id)) {
      const p = this.pending.get(msg.id)!;
      this.pending.delete(msg.id);
      if (msg.error) p.reject(msg.error);
      else p.resolve(msg.result);
    }
  }

  private send(obj: unknown) {
    try {
      this.port?.postMessage(obj);
    } catch (e) {
      console.warn("[passvalet] postMessage failed", e);
    }
  }

  call<T = unknown>(method: string, params: unknown, timeoutMs = 30000): Promise<T> {
    if (!this.port) return Promise.reject({ code: -32003, message: "not connected" });
    const id = this.nextId++;
    return new Promise<T>((resolve, reject) => {
      const t = setTimeout(() => {
        this.pending.delete(id);
        reject({ code: -32002, message: `timeout: ${method}` });
      }, timeoutMs);
      this.pending.set(id, {
        resolve: (v) => {
          clearTimeout(t);
          resolve(v);
        },
        reject: (e) => {
          clearTimeout(t);
          reject(e);
        },
      });
      this.send({ jsonrpc: "2.0", id, method, params });
    });
  }

  notify(method: string, params: unknown) {
    this.send({ jsonrpc: "2.0", method, params });
  }
}

export function rpcError(code: string, message: string): RpcError {
  return { code: -32000, message, data: { code } };
}
