import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  AuditEntry,
  ExtensionStatus,
  PendingPrompt,
  Playbook,
  ProviderConfig,
  RunEvent,
  RunInfo,
  SecretMeta,
  ServiceInfo,
  Session,
  SettingsView,
  UnlockProviderKind,
  VaultInfo,
} from "./types";

export const api = {
  vaultInfo: () => invoke<VaultInfo>("vault_info"),
  vaultSetup: (provider: UnlockProviderKind) =>
    invoke<{ recovery_key: string }>("vault_setup", { provider }),
  vaultUnlock: () => invoke<void>("vault_unlock"),
  vaultLock: () => invoke<void>("vault_lock"),
  vaultUnlockRecovery: (recoveryKey: string) =>
    invoke<void>("vault_unlock_recovery", { recoveryKey }),
  vaultRebind: (provider: UnlockProviderKind) =>
    invoke<{ recovery_key: string }>("vault_rebind", { provider }),
  vaultRegenerateRecovery: () => invoke<{ recovery_key: string }>("vault_regenerate_recovery"),
  biometricsAvailable: () => invoke<boolean>("biometrics_available"),

  listSecrets: () => invoke<SecretMeta[]>("list_secrets"),
  addSecret: (args: { service: string; key_type: string; value: string; label?: string }) =>
    invoke<SecretMeta>("add_secret", { args }),
  deleteSecret: (id: string) => invoke<void>("delete_secret", { id }),
  updateSecretLabel: (id: string, label: string) => invoke<void>("update_secret_label", { id, label }),
  revealSecret: (service: string, keyType: string) =>
    invoke<string>("reveal_secret", { service, keyType }),
  listServices: () => invoke<ServiceInfo[]>("list_services"),

  listSessions: (includeInactive = false) => invoke<Session[]>("list_sessions", { includeInactive }),
  revokeSession: (id: string) => invoke<void>("revoke_session", { id }),
  revokeAllSessions: () => invoke<number>("revoke_all_sessions"),
  auditLog: (limit = 200, offset = 0) => invoke<AuditEntry[]>("audit_log", { limit, offset }),

  settingsGet: () => invoke<SettingsView>("settings_get"),
  settingsSet: (patch: Partial<{
    provider: ProviderConfig;
    auto_lock_minutes: number;
    passkey_domain: string;
    passkey_username: string;
    extension_ids: string[];
    presence_on_approve: boolean;
  }>) => invoke<void>("settings_set", { patch }),
  setLlmApiKey: (apiKey: string) => invoke<void>("set_llm_api_key", { apiKey }),
  testProvider: () => invoke<{ ok: boolean; model: string; message: string }>("test_provider"),

  promptList: () => invoke<PendingPrompt[]>("prompt_list"),
  promptDecide: (id: string, approve: boolean) => invoke<void>("prompt_decide", { id, approve }),

  extensionStatus: () => invoke<ExtensionStatus>("extension_status"),
  listPlaybooks: () => invoke<Playbook[]>("list_playbooks"),
  startCollection: (service: string, keyTypes: string[], hints?: string[]) =>
    invoke<string>("start_collection", { service, keyTypes, hints }),
  startRotation: (service: string, keyType: string) =>
    invoke<string>("start_rotation", { service, keyType }),
  listRuns: () => invoke<RunInfo[]>("list_runs"),
  abortRun: (runId: string) => invoke<boolean>("abort_run", { runId }),
  resumeRun: (runId: string) => invoke<boolean>("resume_run", { runId }),
  clearRuns: () => invoke<void>("clear_runs"),

  installExtensionHost: (extensionId: string) =>
    invoke<string>("install_extension_host", { extensionId }),
  mcpConfigSnippet: () =>
    invoke<{ cursor: string; claude_code: string; codex: string; cli: string }>("mcp_config_snippet"),
  openMainWindow: () => invoke<void>("open_main_window"),
};

export function onVaultChanged(cb: () => void): Promise<UnlistenFn> {
  return listen("vault:changed", () => cb());
}
export function onPromptChanged(cb: () => void): Promise<UnlistenFn> {
  return listen("prompt:changed", () => cb());
}
export function onRunEvent(cb: (ev: RunEvent) => void): Promise<UnlistenFn> {
  return listen<RunEvent>("run:event", (e) => cb(e.payload));
}
export function onExtensionChanged(cb: (connected: boolean) => void): Promise<UnlistenFn> {
  return listen<boolean>("extension:changed", (e) => cb(e.payload));
}

export function errorText(e: unknown): string {
  if (typeof e === "string") return e;
  if (e && typeof e === "object" && "message" in e) return String((e as { message: unknown }).message);
  return String(e);
}

export function fmtTime(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleString("zh-CN", { hour12: false });
}

export function relTime(iso: string): string {
  const diff = (new Date(iso).getTime() - Date.now()) / 1000;
  const abs = Math.abs(diff);
  const unit = abs < 60 ? [abs, "秒"] : abs < 3600 ? [abs / 60, "分钟"] : abs < 86400 ? [abs / 3600, "小时"] : [abs / 86400, "天"];
  const n = Math.round(unit[0] as number);
  return diff < 0 ? `${n}${unit[1]}前` : `${n}${unit[1]}后`;
}
