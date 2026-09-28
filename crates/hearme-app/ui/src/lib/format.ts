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

/** SQLite's local "YYYY-MM-DD HH:MM:SS" → "today 20:10" or "Sep 25 09:15". */
export function when(s: string, now = new Date()): string {
  const m = s.match(/^(\d{4})-(\d\d)-(\d\d) (\d\d):(\d\d)/);
  if (!m) return s;
  const [, y, mo, d, h, mi] = m.map(Number);
  const today = y === now.getFullYear() && mo === now.getMonth() + 1 && d === now.getDate();
  const hh = String(h).padStart(2, "0");
  const mm = String(mi).padStart(2, "0");
  return `${today ? "today" : `${MON[(mo ?? 1) - 1]} ${d}`} ${hh}:${mm}`;
}

export const sameKeys = (a: string[], b: string[]) =>
  a.length === b.length && a.every((x) => b.includes(x));

/** 148 → "148 MB", 1000 → "1 GB", 1500 → "1.5 GB" */
export function megabytes(mb: number): string {
  if (mb < 1000) return `${mb} MB`;
  return `${Number((mb / 1000).toFixed(1))} GB`;
}
