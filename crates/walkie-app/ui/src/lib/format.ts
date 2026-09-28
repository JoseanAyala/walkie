// Small pure helpers shared by the windows (and unit-tested).

const GLYPH: Record<string, string> = {
  Fn: "fn",
  Cmd: "⌘",
  Opt: "⌥",
  Ctrl: "⌃",
  Shift: "⇧",
  Space: "space",
  Return: "⏎",
  Tab: "⇥",
  Escape: "esc",
  Delete: "⌫",
};

/** A key name from the config ("RightOpt", "Space", "V") as shown to people. */
export function glyph(name: string): string {
  const m = name.match(/^(Left|Right)(Cmd|Opt|Ctrl|Shift)$/);
  if (m?.[1] && m[2]) return `${m[1].toLowerCase()} ${GLYPH[m[2]]}`;
  return GLYPH[name] ?? name;
}

export const chord = (keys: string[]) => (keys.length ? keys.map(glyph).join(" ") : "off");

/** "um, uh ,, like" → ["um", "uh", "like"] */
export const words = (v: string) =>
  v
    .split(",")
    .map((s) => s.trim())
    .filter(Boolean);

const MON = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

// SQLite's local "YYYY-MM-DD HH:MM:SS"
const STAMP = /^(\d{4})-(\d\d)-(\d\d) (\d\d:\d\d)/;

/** A timestamp's day: "Today", "Yesterday", "Sep 25", or "Sep 25 2025" in
 * another year. Passes through what it can't parse. */
export function day(s: string, now = new Date()): string {
  const m = s.match(STAMP);
  if (!m) return s;
  const [y, mo, d] = [m[1], m[2], m[3]].map(Number) as [number, number, number];
  const on = (t: Date) => y === t.getFullYear() && mo === t.getMonth() + 1 && d === t.getDate();
  if (on(now)) return "Today";
  if (on(new Date(now.getFullYear(), now.getMonth(), now.getDate() - 1))) return "Yesterday";
  const md = `${MON[mo - 1]} ${d}`;
  return y === now.getFullYear() ? md : `${md} ${y}`;
}

/** A timestamp's time of day, "20:10"; "" if it can't be parsed. */
export const clock = (s: string) => s.match(STAMP)?.[4] ?? "";

/** Consecutive rows under the day they were made, newest-first order kept. */
export function byDay<T extends { created_at: string }>(rows: T[], now = new Date()) {
  const out: { day: string; rows: T[] }[] = [];
  for (const r of rows) {
    const d = day(r.created_at, now);
    const last = out.at(-1);
    if (last?.day === d) last.rows.push(r);
    else out.push({ day: d, rows: [r] });
  }
  return out;
}

export const sameKeys = (a: string[], b: string[]) =>
  a.length === b.length && a.every((x) => b.includes(x));

/** 148 → "148 MB", 1000 → "1 GB", 1500 → "1.5 GB" */
export function megabytes(mb: number): string {
  if (mb < 1000) return `${mb} MB`;
  return `${Number((mb / 1000).toFixed(1))} GB`;
}
