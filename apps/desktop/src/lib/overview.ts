import type { AuditEntry } from "./types";

export const AUDIT_LIMIT = 500;
// A successful agent read has a session. Manual reveal/copy is not agent usage.
export function recentUsage(entries: AuditEntry[], now: number) {
  const today = new Date(now);
  const days = Array.from({ length: 7 }, (_, i) => {
    const start = new Date(today.getFullYear(), today.getMonth(), today.getDate() - 6 + i);
    const end = new Date(start.getFullYear(), start.getMonth(), start.getDate() + 1);
    return { start: start.getTime(), end: end.getTime(), label: `${start.getMonth() + 1}/${start.getDate()}`, count: 0 };
  });
  const reads = entries.filter((e) => {
    const ts = Date.parse(e.ts);
    return e.event === "key_read" && !!e.session_id && ts >= days[0].start && ts <= now;
  }).sort((a, b) => Date.parse(b.ts) - Date.parse(a.ts));
  for (const e of reads) {
    const day = days.find((d) => Date.parse(e.ts) >= d.start && Date.parse(e.ts) < d.end);
    if (day) day.count++;
  }
  return {
    days,
    count: reads.length,
    recent: reads.slice(0, 5).map(({ id, ts, agent }) => ({ id, ts, agent })),
    limited: entries.length >= AUDIT_LIMIT,
  };
}
