// Mirrors the Rust models in passvalet-core / passvalet-agent / desktop commands.

export type Access = "read" | "read_write";

export interface AgentInfo {
  name: string;
  id?: string | null;
  client?: string | null;
}

export interface KeyRequest {
  service: string;
  key_type: string;
  access: Access;
  reason?: string | null;
}

export interface PermissionManifest {
  agent: AgentInfo;
  purpose: string;
  requests: KeyRequest[];
  ttl_seconds?: number | null;
  project?: string | null;
}

export interface Grant {
  service: string;
  key_type: string;
  access: Access;
}

export interface Session {
  id: string;
  agent: AgentInfo;
  purpose: string;
  grants: Grant[];
  project?: string | null;
  created_at: string;
  expires_at: string;
  revoked: boolean;
  last_used_at?: string | null;
  read_count: number;
}

export type SecretSource = "manual" | "collected" | "rotated" | "imported";

export interface SecretMeta {
  id: string;
  service: string;
  key_type: string;
  label?: string | null;
  source: SecretSource;
  fingerprint: string;
  created_at: string;
  updated_at: string;
  last_rotated_at?: string | null;
  metadata: Record<string, unknown>;
}

export type AuditEvent =
  | "vault_initialized"
  | "vault_unlocked"
  | "vault_locked"
  | "secret_added"
  | "secret_updated"
  | "secret_deleted"
  | "session_requested"
  | "session_approved"
  | "session_denied"
  | "session_revoked"
  | "key_read"
  | "key_denied"
  | "key_reported_invalid"
  | "rotation_started"
  | "rotation_completed"
  | "rotation_failed"
  | "collection_started"
  | "collection_completed"
  | "collection_aborted"
  | "recovery_used";

export interface AuditEntry {
  id: number;
  ts: string;
  event: AuditEvent;
  agent?: string | null;
  service?: string | null;
  key_type?: string | null;
  session_id?: string | null;
  detail?: string | null;
}

export type UnlockProviderKind = "passkey_prf" | "touchid_keychain";

export interface VaultInfo {
  initialized: boolean;
  locked: boolean;
  provider?: UnlockProviderKind | null;
  created_at?: string | null;
  secret_count: number;
  active_session_count: number;
  has_recovery: boolean;
  credential_id?: string | null;
}

export interface ManifestLine {
  service: string;
  service_label: string;
  key_type: string;
  key_label: string;
  access: Access;
  available: boolean;
  reason?: string | null;
}

export interface ManifestSummary {
  agent_label: string;
  purpose: string;
  ttl_seconds: number;
  ttl_label: string;
  lines: ManifestLine[];
}

export type PromptKind =
  | { kind: "authorize" }
  | {
      kind: "rotate";
      service: string;
      key_type: string;
      status_code?: number | null;
      message?: string | null;
    };

export interface PendingPrompt {
  id: string;
  kind: PromptKind;
  manifest: PermissionManifest;
  summary: ManifestSummary;
  created_at: string;
  vault_locked: boolean;
}

export interface KeyTypeInfo {
  id: string;
  label: string;
  label_zh: string;
  env_var: string;
  sensitive: boolean;
  pattern?: string | null;
}

export interface ServiceInfo {
  id: string;
  label: string;
  dashboard_url: string;
  key_types: KeyTypeInfo[];
}

export type ProviderKind = "openai_compat" | "anthropic";

export interface ProviderConfig {
  kind: ProviderKind;
  base_url: string;
  api_key?: string | null;
  models: string[];
}

export interface Settings {
  provider: ProviderConfig;
  auto_lock_minutes: number;
  passkey_domain: string;
  passkey_username: string;
  extension_ids: string[];
  presence_on_approve: boolean;
  onboarding_done: boolean;
}

export interface SettingsView extends Settings {
  has_llm_api_key: boolean;
  llm_api_key_preview?: string | null;
  cli_path: string;
  socket_path: string;
  app_dir: string;
}

export interface Playbook {
  service: string;
  label: string;
  start_url: string;
  login_url?: string | null;
  key_types: string[];
  max_steps: number;
  collect: { instructions: string };
  rotate?: { supported: boolean; max_steps: number; instructions: string; key_types: string[] } | null;
}

export type RunKind = { kind: "collect"; key_types: string[] } | { kind: "rotate"; key_type: string };

export type RunOutcome =
  | { status: "success"; captured: string[]; summary: string }
  | { status: "partial"; captured: string[]; missing: string[]; reason: string }
  | { status: "failed"; reason: string }
  | { status: "aborted" };

export type RunEvent =
  | { type: "started"; run_id: string; service: string; model: string }
  | { type: "thought"; run_id: string; text: string }
  | { type: "step"; run_id: string; step: number; tool: string; args: string; result: string; is_error: boolean }
  | { type: "captured"; run_id: string; key_type: string; fingerprint: string; valid: boolean }
  | { type: "escalated"; run_id: string; from: string; to: string }
  | { type: "need_user"; run_id: string; message: string }
  | { type: "finished"; run_id: string; outcome: RunOutcome };

export interface RunInfo {
  run_id: string;
  service: string;
  kind: RunKind;
  started_at: string;
  finished?: RunOutcome | null;
  events: RunEvent[];
}

export interface ExtensionStatus {
  connected: boolean;
  info?: { extension_version: string; browser: string } | null;
}
