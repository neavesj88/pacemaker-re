// Mock of the settings-preset commands in src-tauri/src/commands/device/preset.rs. Invented data
// only. The diff is a simplified copy of pm_formats::ops_preset::diff over the mock card's
// settings (devmock/device.ts), so the browser preview shows the same statuses and wording as the
// app. Picked up automatically by devmock/index.ts.
//
// Switches in the drive's name: "nosettings" is a card without settings.dat. A file path that
// contains "bad" is not a preset; one that contains "taken" imports under a name that is stored.
import { mockSettings } from "./device";
import type { Handlers } from "./types";

type S = ReturnType<typeof mockSettings>[number];
type PSetting = { id: number; group: string; title: string; kind: "bool" | "list"; value: number; label: string };
type Preset = { format: string; version: number; name: string; saved_utc: string; firmware: string | null; settings: PSetting[] };

const FORMAT = "pacemaker-revival-settings-preset";
const key = (n: string) => n.split(/\s+/).filter(Boolean).join(" ").toLowerCase();
const clean = (n: string) => {
  const c = n.split(/\s+/).filter(Boolean).join(" ");
  if (!c) throw "invalid: a preset needs a name";
  if ([...c].length > 80) throw "invalid: a preset name is at most 80 characters";
  return c;
};
const has = (a: Record<string, unknown>, word: string) => String(a.root ?? "").toLowerCase().includes(word);
const label = (s: S, v: number) => s.options[v] ?? `#${v}`;
const stamp = () => new Date().toISOString().replace(/[-:]/g, "").replace(/\.\d+Z$/, "Z");

const entry = (id: number, group: string, title: string, kind: "bool" | "list", value: number, text: string): PSetting => ({
  id, group, title, kind, value, label: text,
});

// a preset made on "another card": differs from the mock card in a few places, mentions a
// setting the mock file does not hold yet (2002) and one the editor does not know (9999)
const store: Preset[] = [
  {
    format: FORMAT, version: 1, name: "Quiet bedroom", saved_utc: "20261108T201500Z", firmware: "180096069",
    settings: [
      entry(2001, "Power", "Backlight intensity", "list", 0, "Low"),
      entry(2003, "Power", "Nightlight", "bool", 1, "On"),
      entry(2000, "Power", "Standby", "list", 2, "5 min"),
      entry(1300, "Mix control", "Autocue", "bool", 1, "On"),
      entry(1904, "FX pads", "P-up / North", "list", 4, "Reverb"),
      entry(2002, "Power", "Backlight timeout", "list", 1, "10 sec"),
      entry(9999, "Other", "Unlisted setting", "bool", 1, "On"),
    ],
  },
];

function fromCard(name: string): Preset {
  const settings = mockSettings()
    .filter((s) => s.present && s.editable && (s.kind === "bool" || s.kind === "list"))
    .map((s) => entry(s.id, s.group, s.title, s.kind as "bool" | "list", s.value, label(s, s.value)));
  return { format: FORMAT, version: 1, name: clean(name), saved_utc: stamp(), firmware: "180096069", settings };
}

function summary(p: Preset) {
  return { name: p.name, file: `${key(p.name).replace(/[^a-z0-9]+/g, "-")}-0000000000000000.json`, settings: p.settings.length, saved_utc: p.saved_utc, firmware: p.firmware };
}

function find(name: string): Preset {
  const p = store.find((x) => key(x.name) === key(name));
  if (!p) throw `there is no preset called "${name}"`;
  return p;
}

function put(p: Preset, overwrite: boolean) {
  const i = store.findIndex((x) => key(x.name) === key(p.name));
  if (i >= 0 && !overwrite) return { saved: false, exists: true, summary: null };
  if (i >= 0) store[i] = p;
  else store.push(p);
  return { saved: true, exists: false, summary: summary(p) };
}

function diff(p: Preset) {
  const card = mockSettings();
  const rows = [] as {
    id: number; group: string; title: string; status: string; current: string | null; wanted: string; detail: string | null;
  }[];
  for (const c of card) {
    const w = p.settings.find((x) => x.id === c.id);
    if (!w) continue;
    let status = "change";
    let detail: string | null = null;
    if (!c.present) [status, detail] = ["absent", "this card's settings file does not hold it yet; the device adds it when it saves"];
    else if (c.kind !== w.kind) [status, detail] = ["invalid", `the preset says ${w.kind}, this setting is a ${c.kind}`];
    else if (!c.editable) [status, detail] = ["invalid", "the editor cannot change this setting"];
    else if (w.value < 0 || !Number.isInteger(w.value) || w.value >= c.options.length) {
      [status, detail] = ["invalid", `the value ${w.value} is not one of this setting's ${c.options.length} options`];
    } else if (label(c, w.value) !== w.label) {
      [status, detail] = ["invalid", `the preset calls option ${w.value} "${w.label}"; this editor calls it "${label(c, w.value)}", so the meaning may differ`];
    } else if (c.value === w.value) status = "same";
    rows.push({ id: c.id, group: c.group, title: c.title, status, current: c.present ? label(c, c.value) : null, wanted: w.label, detail });
  }
  for (const w of p.settings) {
    if (!card.some((c) => c.id === w.id)) {
      rows.push({
        id: w.id, group: w.group, title: w.title, status: "unknown", current: null, wanted: w.label,
        detail: "this editor does not know a setting with this number",
      });
    }
  }
  const n = (s: string) => rows.filter((r) => r.status === s).length;
  const untouched = card.filter((c) => c.present && c.editable && !p.settings.some((w) => w.id === c.id)).length;
  return { name: p.name, rows, change: n("change"), same: n("same"), blocked: n("absent") + n("unknown") + n("invalid"), untouched };
}

// what a picked file holds: a made-up preset, named like a stored one when the path says "taken"
function fileOf(path: string): Preset {
  if (path.toLowerCase().includes("bad")) throw "invalid: not a settings preset: expected value at line 1 column 1";
  return {
    format: FORMAT, version: 1, name: path.toLowerCase().includes("taken") ? store[0].name : "Imported demo",
    saved_utc: "20261101T090000Z", firmware: "180096069",
    settings: [
      entry(2001, "Power", "Backlight intensity", "list", 2, "Bright"),
      entry(2004, "Power", "Buttons blink", "bool", 0, "Off"),
      entry(1300, "Mix control", "Autocue", "bool", 0, "Off"),
    ],
  };
}

const noSettings = (a: Record<string, unknown>) => {
  if (has(a, "nosettings")) throw "this card has no settings file yet; the Pacemaker creates it the first time it is shut down";
};

export const handlers: Handlers = {
  device_presets_list: () => ({
    presets: [...store].sort((a, b) => key(a.name).localeCompare(key(b.name))).map(summary),
    unreadable: [],
  }),

  device_preset_get: (a) => find(String(a.name)),

  device_preset_save_current: (a) => {
    noSettings(a);
    return put(fromCard(String(a.name)), !!a.overwrite);
  },

  device_preset_delete: (a) => {
    const i = store.findIndex((x) => key(x.name) === key(String(a.name)));
    if (i >= 0) store.splice(i, 1);
    return i >= 0;
  },

  device_preset_export: (a) => {
    find(String(a.name));
    if (!String(a.dest ?? "").trim()) throw "choose where to save the preset";
  },

  device_preset_read_file: (a) => fileOf(String(a.path)),

  device_preset_import: (a) => {
    const base = fileOf(String(a.path));
    if (a.name != null) base.name = clean(String(a.name));
    return put(base, !!a.overwrite);
  },

  device_preset_diff: (a) => {
    noSettings(a);
    return diff(find(String(a.name)));
  },

  device_preset_apply: (a) => {
    noSettings(a);
    const p = find(String(a.name));
    const d = diff(p);
    const written: number[] = [];
    for (const id of new Set(a.ids as number[])) {
      const r = d.rows.find((x) => x.id === id);
      if (!r) throw `invalid: setting ${id} is not in the preset`;
      if (r.status === "same") continue;
      if (r.status !== "change") throw `invalid: ${r.title} cannot be applied: ${r.detail}`;
      written.push(id);
    }
    const card = mockSettings();
    for (const id of written) {
      const c = card.find((x) => x.id === id)!;
      c.value = p.settings.find((x) => x.id === id)!.value;
      c.changed = c.value !== c.default;
    }
    return { changed: written.length, written };
  },
};
