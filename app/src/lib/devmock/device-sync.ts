// Mock of the library sync commands in src-tauri/src/commands/device/sync.rs. Invented data only.
// It simulates two libraries so the sync UI can be built in the browser preview: "This computer"
// (COMPUTER_ROOT) and the mock device (P:\). Sending (computer -> device) has something of every
// kind to show: copies, an update, a mix that is skipped with a reason, a replaced mix, a case
// merge and a new case. Getting (device -> computer) has less. What a run finishes stops being
// planned (it shows "already the same"), so a second preview after a sync reads as nothing to do.
// Picked up automatically by devmock/index.ts.
//
// Switches in a library root's name: "full" has too little free room.
import { libraryKind } from "../state/place";
import { DEFAULT as COMPUTER_DEFAULT } from "./computerLibrary";
import { runSteps } from "./device";
import type { Handlers } from "./types";

/** The same library Settings and Welcome create (computerLibrary.ts): one computer library in the preview. */
export const COMPUTER_ROOT = COMPUTER_DEFAULT;
export const DEVICE_ROOT = "P:\\";

const MB = 1024 * 1024;
const SAME = "already the same";

type Action = { type: string; name?: string; reason?: string };
type Need = { location: string; label: string };
type Item = {
  key: string; kind: "track" | "mix" | "case"; action: Action; label: string; bytes: number; needs: Need[]; detail: string;
  global_id: string | null; location: string | null; source_id: number | null; target_id: number | null;
  blockers: { key: string; label: string }[];
};

type Settings = {
  tracks_on_both: "newer" | "leave" | "source"; mixes_same_name: "newer" | "replace" | "keep_both" | "skip";
  cases_same_name: "merge" | "replace" | "skip"; remove_missing: boolean; play_counts: "higher" | "leave"; labels_follow: boolean;
};

let settings: Settings = {
  tracks_on_both: "newer", mixes_same_name: "newer", cases_same_name: "merge", remove_missing: false, play_counts: "higher", labels_follow: true,
};

/** Items a finished run has already handled, per direction. */
const done = new Map<string, Set<string>>();

const loc = (n: number) => `/pmdb_tracks/${n.toString(16)[0]}/${(n * 7).toString(16)[0]}/${(0x2f00 + n * 977).toString(16)}.mp3`;
const track = (n: number, label: string, mb: number, action: Action, extra: Partial<Item> = {}): Item => ({
  key: `track:${loc(n)}`, kind: "track", action, label, bytes: Math.round(mb * MB), needs: [], detail: "",
  global_id: `mock-global-${n}`, location: loc(n), source_id: n, target_id: action.type === "update" || action.type === "skip" ? n + 100 : null, blockers: [], ...extra,
});
const need = (n: number, label: string): Need => ({ location: loc(n), label });

const NIGHT = need(1, "Example Artist - Night Drive");
const GLASS = need(2, "Example Artist - Glass Rooms");
const LIGHTS = need(3, "Sample Band - Slow Lights");
const MOON = need(4, "Sample Band - Paper Moon");
const MISSING = need(9, "Sample Band - Lost Reel");

function send(): Item[] {
  const items: Item[] = [
    track(1, NIGHT.label, 8.4, { type: "copy" }),
    track(2, GLASS.label, 9.1, { type: "copy" }),
    track(3, LIGHTS.label, 7.7, { type: "copy" }),
    track(4, MOON.label, 10.3, { type: "copy" }),
    track(5, "Demo Duo - Evening Train", 0.01, { type: "update" }),
    track(10, "Demo Duo - Dusk", 6.4, { type: "repair" }, { target_id: 110, detail: "the audio file is missing on the target" }),
    track(11, "Sample Band - Lantern", 0.01, { type: "repair" }, { target_id: 111, detail: "the beat grid file (.str) is missing on the target" }),
    track(12, "Sample Band - Harbour Lights", 0, { type: "update_plays" }, { target_id: 112, detail: "played more on the sending side" }),
    track(13, "Sample Band - Quiet Hours", 0, { type: "skip", reason: "the beat grid file (.str) is missing on both sides" }, { target_id: 113 }),
    track(9, MISSING.label, 0, { type: "skip", reason: "its audio file is missing" }),
    track(6, "Demo Duo - Tidal", 0, { type: "skip", reason: SAME }),
    track(7, "Demo Duo - Morning Light", 0, { type: "skip", reason: SAME }),
    track(8, "Example Artist - Low Tide", 0, { type: "skip", reason: "the target's copy is the same age or newer" }),
    {
      key: "mix:friday warm-up", kind: "mix", action: { type: "copy" }, label: "Friday warm-up", bytes: 0, needs: [NIGHT, GLASS, LIGHTS],
      detail: "", global_id: null, location: null, source_id: 1, target_id: null, blockers: [],
    },
    {
      key: "mix:road trip", kind: "mix", action: { type: "skip", reason: `it needs "${MISSING.label}", which will not be on the target` }, label: "Road trip",
      bytes: 0, needs: [NIGHT, MISSING], detail: "", global_id: null, location: null, source_id: 2, target_id: null, blockers: [],
    },
    {
      key: "mix:sunday chill", kind: "mix", action: { type: "replace" }, label: "Sunday chill", bytes: 0, needs: [MOON],
      detail: "", global_id: null, location: null, source_id: 3, target_id: 14, blockers: [],
    },
    {
      key: "case:opening tracks", kind: "case", action: { type: "merge" }, label: "Opening tracks", bytes: 0, needs: [NIGHT, GLASS, MOON],
      detail: "", global_id: null, location: null, source_id: 1, target_id: 3, blockers: [],
    },
    {
      key: "case:new finds", kind: "case", action: { type: "copy" }, label: "New finds", bytes: 0, needs: [LIGHTS, MOON, MISSING],
      detail: "1 of 3 tracks will not be there and are left out", global_id: null, location: null, source_id: 2, target_id: null, blockers: [],
    },
  ];
  return items;
}

function get(): Item[] {
  return [
    track(21, "Demo Duo - Back Roads", 6.2, { type: "copy" }),
    track(22, "Sample Band - Harbour", 8.8, { type: "copy" }),
    track(23, "Demo Duo - Tidal", 0, { type: "skip", reason: SAME }),
    {
      key: "case:on the go", kind: "case", action: { type: "copy" }, label: "On the go", bytes: 0, needs: [need(21, "Demo Duo - Back Roads"), need(22, "Sample Band - Harbour")],
      detail: "", global_id: null, location: null, source_id: 5, target_id: null, blockers: [],
    },
  ];
}

/** The removals the "remove what the other side lacks" setting adds. */
function removals(): Item[] {
  return [
    track(30, "Old Band - Unused Demo", 0, { type: "remove" }, { key: `remove-track:${loc(30)}`, source_id: null, target_id: 130 }),
    track(31, "Old Band - Still In A Case", 0, { type: "skip", reason: 'it is used by case "Keep"' }, { key: `remove-track:${loc(31)}`, source_id: null, target_id: 131 }),
  ];
}

const isSend = (a: Record<string, any>) => libraryKind(String(a.source)) === "computer";

function currentPlan(a: Record<string, any>) {
  const dir = isSend(a) ? "send" : "get";
  const handled = done.get(dir) ?? new Set<string>();
  let items = (isSend(a) ? send() : get()).map((i) => (handled.has(i.key) && i.action.type !== "skip" ? { ...i, bytes: 0, action: { type: "skip", reason: SAME } } : i));
  if (settings.remove_missing) items = [...items, ...removals().filter((i) => !handled.has(i.key) || i.action.type === "skip")];
  if (settings.tracks_on_both === "leave") items = items.map((i) => (i.action.type === "update" ? { ...i, bytes: 0, action: { type: "skip", reason: "both have it and the setting leaves it as it is" } } : i));
  if (settings.mixes_same_name === "skip") {
    items = items.map((i) => (i.kind === "mix" && i.action.type === "replace" ? { ...i, action: { type: "skip", reason: "a mix with this name is already there and the setting skips it" } } : i));
  }
  if (settings.mixes_same_name === "keep_both") {
    items = items.map((i) => (i.kind === "mix" && i.action.type === "replace" ? { ...i, action: { type: "copy_as_new", name: `${i.label} (from ${isSend(a) ? "computer" : "Pacemaker"})` } } : i));
  }
  const bytes = items.filter((i) => i.kind === "track" && i.action.type !== "skip").reduce((n, i) => n + i.bytes, 0);
  const full = String(a.target).toLowerCase().includes("full");
  const free = full ? 5 * MB : libraryKind(String(a.target)) === "computer" ? 212_000 * MB : 11_400 * MB;
  const { labels_follow: _l, ...engine } = settings;
  const digest = `mock-${dir}-${JSON.stringify(engine)}-${[...handled].sort().join("|")}`;
  return {
    items, settings: engine, source_label: isSend(a) ? "computer" : "Pacemaker", bytes_to_copy: bytes, free_bytes: free,
    fits: bytes === 0 || bytes + MB <= free, digest, notes: ["Smart cases are kept by the app on this computer and are not synced."],
  };
}

const plural = (n: number, one: string) => `${n} ${one}${n === 1 ? "" : one === "mix" ? "es" : "s"}`;

export const handlers: Handlers = {
  sync_settings_get: () => ({ settings: { ...settings }, warning: null }),
  sync_settings_set: (a) => {
    settings = { ...(a.settings as Settings) };
    return { settings: { ...settings }, warning: null };
  },

  sync_plan: (a) => {
    if (a.source === a.target) throw "invalid: the source and the target are the same library";
    return currentPlan(a);
  },

  sync_apply: (a) => {
    const plan = currentPlan(a);
    if (plan.digest !== a.digest) throw "The libraries changed since the preview, so nothing was synced. Look at the preview again.";
    const excluded = new Set<string>(a.excluded as string[]);
    const left = new Set(plan.items.filter((i) => excluded.has(i.key) && i.action.type !== "skip").map((i) => i.key));
    const acting = plan.items.filter((i) => i.action.type !== "skip" && !left.has(i.key));
    // a mix or case that needs a track the user unticked is left out, like the engine does for mixes
    const dropped = acting.filter((i) => i.kind === "mix" && i.needs.some((n) => plan.items.some((t) => t.location === n.location && left.has(t.key))));
    const run = acting.filter((i) => !dropped.includes(i));
    const bytes = run.filter((i) => i.kind === "track").reduce((n, i) => n + i.bytes, 0);
    const copied = (k: string, t: string) => run.filter((i) => i.kind === k && (i.action.type === t || (t === "copy" && i.action.type === "copy_as_new"))).length;
    const parts: string[] = [];
    const tc = copied("track", "copy"), mc = copied("mix", "copy"), cc = copied("case", "copy");
    if (tc + mc + cc > 0) {
      const what = [tc ? `${plural(tc, "track")} (${(bytes / MB).toFixed(1)} MB)` : "", mc ? plural(mc, "mix") : "", cc ? plural(cc, "case") : ""].filter(Boolean);
      parts.push(`Copied ${what.join(", ")}`);
    }
    if (copied("track", "update")) parts.push(`Updated ${plural(copied("track", "update"), "track")}`);
    if (copied("track", "repair")) parts.push(`Repaired the missing files of ${plural(copied("track", "repair"), "track")}`);
    const playsN = run.filter((i) => i.action.type === "update_plays").length;
    if (playsN) parts.push(`Updated the play counts of ${plural(playsN, "item")}`);
    if (copied("mix", "replace")) parts.push(`Replaced ${plural(copied("mix", "replace"), "mix")}`);
    if (copied("case", "merge")) parts.push(`Merged ${plural(copied("case", "merge"), "case")}`);
    const removed = run.filter((i) => i.action.type === "remove").length;
    if (removed) parts.push(`Removed ${removed} items that the other side does not have`);
    const owed = [...dropped.map((i) => `${i.label}: it needs tracks that were left out`), ...[...left].map((k) => `${plan.items.find((i) => i.key === k)?.label}: left out by you`)];
    if (owed.length) parts.push(`${plural(owed.length, "item")} left out: ${owed.join("; ")}`);
    const summary = parts.join(". ") || "Nothing to sync, the two libraries already match";
    const labels = run.map((i) => i.label);
    const handled = done.get(isSend(a) ? "send" : "get") ?? new Set<string>();
    runSteps("device-sync", labels.length ? labels : ["Checking"], "syncing", summary, () => run.forEach((i) => handled.add(i.key)), (d, t) => `${((bytes * d) / Math.max(t, 1) / MB).toFixed(1)} of ${(bytes / MB).toFixed(1)} MB`);
    done.set(isSend(a) ? "send" : "get", handled);
  },
};
