// Extension-side masking. The desktop app redacts again; this keeps secrets out of extension
// logs and out of the text that travels through the native host.

const PATTERNS: RegExp[] = [
  /sk-ant-[A-Za-z0-9_-]{20,}/g,
  /sk-proj-[A-Za-z0-9_-]{20,}/g,
  /sk-[A-Za-z0-9_-]{20,}/g,
  /(?:sk|rk|pk)_(?:live|test)_[A-Za-z0-9]{16,}/g,
  /whsec_[A-Za-z0-9]{16,}/g,
  /sb_(?:secret|publishable)_[A-Za-z0-9_-]{16,}/g,
  /eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}/g,
  /ghp_[A-Za-z0-9]{30,}/g,
  /gho_[A-Za-z0-9]{30,}/g,
  /github_pat_[A-Za-z0-9_]{40,}/g,
  /AKIA[A-Z0-9]{16}/g,
  /AIza[A-Za-z0-9_-]{30,}/g,
  /re_[A-Za-z0-9_]{20,}/g,
  /xox[abpr]-[A-Za-z0-9-]{20,}/g,
  /glpat-[A-Za-z0-9_-]{20,}/g,
];

export function mask(value: string): string {
  const chars = [...value];
  if (chars.length <= 8) return "•".repeat(Math.max(4, chars.length));
  return `${chars.slice(0, 4).join("")}…${chars.slice(-4).join("")}`;
}

export function redact(text: string): string {
  let out = text;
  for (const re of PATTERNS) {
    out = out.replace(re, (m) => `[REDACTED ${mask(m)}]`);
  }
  // generic: long opaque tokens (32+ base64/hex chars, has digits) next to key-ish words
  out = out.replace(
    /((?:key|secret|token|password|credential)[^\n]{0,40}?)\b([A-Za-z0-9_\-/+=]{32,})/gi,
    (whole, prefix: string, tok: string) => (/\d/.test(tok) && !tok.includes("://") ? `${prefix}[REDACTED ${mask(tok)}]` : whole),
  );
  return out;
}
