// Pure helpers of the settings presets (PresetsCard, PresetApplyModal). No imports from the app,
// so `node --experimental-strip-types` runs them in presetLogic.test.mjs.

export type RowStatus = "change" | "same" | "absent" | "unknown" | "invalid";

/** Mirrors api/device-preset.ts PresetDiffRow. */
export type Row = {
  id: number;
  group: string;
  title: string;
  status: RowStatus;
  current: string | null;
  wanted: string;
  detail: string | null;
};

export type Counts = { change: number; same: number; blocked: number; untouched: number };

const plural = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** Rows in groups, in the order each group first appears. */
export function groupRows<T extends { group: string }>(rows: T[]): { group: string; rows: T[] }[] {
  const out: { group: string; rows: T[] }[] = [];
  for (const r of rows) {
    let g = out.find((x) => x.group === r.group);
    if (!g) out.push((g = { group: r.group, rows: [] }));
    g.rows.push(r);
  }
  return out;
}

/** The ids that start ticked: every row that would change something. */
export function defaultTicks(rows: Row[]): number[] {
  return rows.filter((r) => r.status === "change").map((r) => r.id);
}

/** The ids to send: ticked and still a change row (a tick on any other row is dropped, never written). */
export function idsToWrite(rows: Row[], ticked: Iterable<number>): number[] {
  const on = new Set(ticked);
  return rows.filter((r) => r.status === "change" && on.has(r.id)).map((r) => r.id);
}

/** What the row says in words. The same text is shown before and after the user ticks it. */
export function rowText(r: Row): string {
  switch (r.status) {
    case "change":
      return `${r.current ?? "not set"} → ${r.wanted}`;
    case "same":
      return `already ${r.wanted}`;
    default:
      return `not applied: ${r.detail ?? "it cannot be applied to this card"}`;
  }
}

/** The headline of the preview: what applying does, in counts. */
export function headline(c: Counts, ticked: number): string {
  const parts: string[] = [];
  if (c.change === 0) parts.push("Nothing to change: this card already matches the preset");
  else if (ticked === c.change) parts.push(`${plural(c.change, "setting")} will change`);
  else parts.push(`${ticked} of ${plural(c.change, "change")} ticked`);
  if (c.same) parts.push(`${c.same} already match${c.same === 1 ? "es" : ""}`);
  if (c.blocked) parts.push(`${plural(c.blocked, "entry", "entries")} cannot be applied`);
  if (c.untouched) {
    parts.push(`${plural(c.untouched, "other setting")} on the card ${c.untouched === 1 ? "stays as it is" : "stay as they are"}`);
  }
  return parts.join("; ") + ".";
}

/** The text of the button that writes. */
export function writeLabel(ticked: number, writing: boolean): string {
  if (writing) return "Writing…";
  return ticked === 0 ? "Nothing ticked" : `Write ${plural(ticked, "setting")}`;
}

/** The name a user typed, as the backend will store it (outer spaces off, runs of spaces made one). */
export function cleanName(raw: string): string {
  return raw.split(/\s+/).filter(Boolean).join(" ");
}

/** Whether two names are one preset: same ignoring case and extra spaces (as the backend decides). */
export function sameName(a: string, b: string): boolean {
  return cleanName(a).toLowerCase() === cleanName(b).toLowerCase();
}

/** Why a typed name cannot be used, or "" if it can (the backend checks again). */
export function nameProblem(raw: string): string {
  const n = cleanName(raw);
  if (!n) return "Give the preset a name.";
  if ([...n].length > 80) return "A preset name is at most 80 characters.";
  if ([...n].some((c) => c.charCodeAt(0) < 32 || c.charCodeAt(0) === 127)) return "A preset name cannot contain control characters.";
  return "";
}

/** "5 settings" / "1 setting" with the version and time of the card it came from, if known. */
export function presetMeta(p: { settings: number; firmware: string | null }, saved: string): string {
  const bits = [plural(p.settings, "setting")];
  if (saved) bits.push(`saved ${saved}`);
  if (p.firmware) bits.push(`from software ${p.firmware}`);
  return bits.join(" · ");
}
