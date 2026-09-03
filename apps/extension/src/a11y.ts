// Accessibility tree → tagged text, plus ref bookkeeping.

export interface AXNodeRaw {
  nodeId: string;
  ignored: boolean;
  role?: { value: string };
  name?: { value: string };
  value?: { value: unknown };
  description?: { value: string };
  properties?: { name: string; value: { value: unknown } }[];
  childIds?: string[];
  parentId?: string;
  backendDOMNodeId?: number;
}

export interface RefEntry {
  backendNodeId: number;
  role: string;
  name: string;
}

const INTERACTIVE = new Set([
  "button",
  "link",
  "textbox",
  "searchbox",
  "checkbox",
  "radio",
  "combobox",
  "listbox",
  "option",
  "menuitem",
  "menuitemcheckbox",
  "menuitemradio",
  "tab",
  "switch",
  "slider",
  "spinbutton",
  "treeitem",
  "gridcell",
  "cell",
  "columnheader",
  "rowheader",
  "row",
  "code",
  "textfield",
]);

const SKIP_ROLES = new Set(["none", "presentation", "generic", "InlineTextBox", "LineBreak"]);

export interface BuildOptions {
  interactiveOnly: boolean;
  maxDepth: number;
  scopeBackendNodeId?: number;
  maxChars: number;
}

export interface Built {
  text: string;
  refs: Map<string, RefEntry>;
  /** flat list for `find` */
  rows: { ref: string | null; role: string; name: string; desc: string; depth: number; line: string }[];
}

export function buildTree(nodes: AXNodeRaw[], opts: BuildOptions, refStart: number): Built {
  const byId = new Map<string, AXNodeRaw>();
  for (const n of nodes) byId.set(n.nodeId, n);
  const roots = nodes.filter((n) => !n.parentId || !byId.has(n.parentId));
  const refs = new Map<string, RefEntry>();
  const rows: Built["rows"] = [];
  let counter = refStart;
  const lines: string[] = [];
  let chars = 0;
  let truncated = false;

  let startNodes = roots;
  if (opts.scopeBackendNodeId !== undefined) {
    const scoped = nodes.find((n) => n.backendDOMNodeId === opts.scopeBackendNodeId);
    if (scoped) startNodes = [scoped];
  }

  const visit = (n: AXNodeRaw, depth: number, indent: number) => {
    if (truncated) return;
    if (depth > opts.maxDepth) return;
    const role = n.role?.value ?? "";
    const name = (n.name?.value ?? "").toString().trim();
    const desc = (n.description?.value ?? "").toString().trim();
    const value = n.value?.value;
    const props = (n.properties ?? [])
      .filter((p) => ["checked", "expanded", "selected", "disabled", "pressed", "required", "invalid", "focused", "readonly", "hidden"].includes(p.name))
      .filter((p) => p.value?.value !== false && p.value?.value !== "false")
      .map((p) => (p.value?.value === true || p.value?.value === "true" ? p.name : `${p.name}=${p.value?.value}`));
    const hidden = props.includes("hidden");
    const isInteractive = INTERACTIVE.has(role) || (role === "generic" && (n.properties ?? []).some((p) => p.name === "focusable" && p.value?.value === true));
    const meaningful = !n.ignored && !hidden && !SKIP_ROLES.has(role) && (isInteractive || name || (value !== undefined && value !== ""));

    let nextIndent = indent;
    if (meaningful && (!opts.interactiveOnly || isInteractive)) {
      let ref: string | null = null;
      if (isInteractive && n.backendDOMNodeId !== undefined) {
        ref = `ref_${++counter}`;
        refs.set(ref, { backendNodeId: n.backendDOMNodeId, role, name });
      }
      const parts: string[] = [];
      if (ref) parts.push(`[${ref}]`);
      parts.push(role || "node");
      if (name) parts.push(JSON.stringify(name.length > 200 ? name.slice(0, 200) + "…" : name));
      if (value !== undefined && value !== "" && value !== null) {
        const v = String(value);
        parts.push(`value=${JSON.stringify(v.length > 120 ? v.slice(0, 120) + "…" : v)}`);
      }
      if (desc) parts.push(`desc=${JSON.stringify(desc.slice(0, 120))}`);
      if (props.length) parts.push(`[${props.join(", ")}]`);
      const line = "  ".repeat(indent) + parts.join(" ");
      chars += line.length + 1;
      if (chars > opts.maxChars) {
        truncated = true;
        return;
      }
      lines.push(line);
      rows.push({ ref, role, name, desc, depth: indent, line: line.trim() });
      nextIndent = indent + 1;
    }
    for (const cid of n.childIds ?? []) {
      const c = byId.get(cid);
      if (c) visit(c, depth + 1, nextIndent);
    }
  };
  for (const r of startNodes) visit(r, 0, 0);

  let text = lines.join("\n");
  if (truncated) text += `\n… output capped at ${opts.maxChars} characters; narrow with filter="interactive", a smaller depth, or a ref.`;
  if (!text) text = "(empty accessibility tree — page may still be loading; try wait then read_page again, or screenshot)";
  return { text, refs, rows };
}

/** Score rows against a natural-language query; returns best matches first. */
export function findMatches(rows: Built["rows"], query: string, limit = 20) {
  const q = query.toLowerCase();
  const tokens = q.split(/[^a-z0-9\u4e00-\u9fff_]+/i).filter((t) => t.length > 1);
  const scored = rows
    .map((r) => {
      const hay = `${r.role} ${r.name} ${r.desc}`.toLowerCase();
      let score = 0;
      if (hay.includes(q)) score += 10;
      for (const t of tokens) {
        if (hay.includes(t)) score += 2;
        if (r.role.toLowerCase() === t) score += 3;
      }
      if (r.ref) score += 1;
      return { r, score };
    })
    .filter((x) => x.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, limit);
  return scored.map((x) => x.r);
}
