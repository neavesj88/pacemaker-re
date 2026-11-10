// Mock of src-tauri/src/commands/device.rs. Invented data only; the "drive" holds 24 demo tracks.
// The long operations (firmware copy, backup, clean-up) emit the same job events as the backend.
import { emit } from "@tauri-apps/api/event";
import type { Handlers } from "./types";

const MB = 1024 * 1024;

const firmware = {
  serial_masked: "***********0042",
  sw_version: "180096069",
  fw_version: "180096069",
  bootloader_version: "12838",
  editor_compatible_flag: "002",
  pending_pfw: [] as string[],
};

const storage = {
  total_bytes: 63_900 * MB,
  free_bytes: 60_600 * MB,
  music_bytes: 3_200 * MB,
  grids_mixes_bytes: 4 * MB,
  other_bytes: 96 * MB,
};

let unreferenced = [
  { path: "3/a/9f2c01de.str", size: 2_104, kind: "grid" },
  { path: "5/1/00b7e4a2.str", size: 2_231, kind: "grid" },
  { path: "7/c/4d19aa03.mp3", size: 6_204_117, kind: "audio" },
  { path: "7/c/Thumbs.db", size: 5_120, kind: "other" },
];

const lists = {
  loop: ["Off", "1/64", "1/32", "1/16", "1/8", "1/4", "1/2", "1", "2", "4"],
  jump: ["1/16", "1/8", "1/4", "1/2", "1", "2", "4", "8"],
  fx: ["Gain", "Mid", "Treble", "Bass", "Reverb", "Echo", "Roll", "Hi-lo cut", "Crush", "Trans", "Wah", "Delay", "Key"],
};

type S = {
  id: number; group: string; title: string; kind: "bool" | "list" | "float"; value: number; default: number;
  options: string[]; editable: boolean; present: boolean; changed: boolean;
};
const bool = (id: number, group: string, title: string, value: boolean, dflt: boolean, off = "Off", on = "On"): S => ({
  id, group, title, kind: "bool", value: +value, default: +dflt, options: [off, on], editable: true, present: true, changed: value !== dflt,
});
const list = (id: number, group: string, title: string, options: string[], value: number, dflt: number): S => ({
  id, group, title, kind: "list", value, default: dflt, options, editable: true, present: true, changed: value !== dflt,
});
const float = (id: number, title: string, value: number, dflt: number): S => ({
  id, group: "Levels (read-only)", title, kind: "float", value, default: dflt, options: [], editable: false, present: true, changed: value !== dflt,
});

let settings: S[] = [
  list(800, "Playback", "Repeat", ["Off", "One", "All"], 0, 0),
  bool(801, "Playback", "Shuffle", false, false),
  bool(1300, "Mix control", "Autocue", true, true),
  list(1301, "Mix control", "Fixed loop length", lists.loop, 0, 0),
  list(1303, "Mix control", "Beat jump length", lists.jump, 3, 3),
  bool(1304, "Mix control", "Loop travel", true, true),
  list(1307, "Mix control", "Effect x-fader", ["Off", "Bass", "Filter"], 0, 0),
  bool(1308, "Mix control", "Phones x-fader", true, true, "Relative", "Normal"),
  bool(1600, "Audio", "Normalize", true, true),
  list(100, "Audio", "Phones range", ["Low impedance", "High impedance"], 0, 0),
  bool(1702, "Track list filters", "Album", false, true),
  bool(1706, "Track list filters", "BPM", true, true),
  bool(1800, "Track time", "Display", false, true, "Left", "Played"),
  list(1900, "FX pads", "North", lists.fx, 2, 2),
  list(1904, "FX pads", "P-up / North", lists.fx, 7, 4),
  list(2000, "Power", "Standby", ["Off", "1 min", "5 min"], 2, 2),
  list(2001, "Power", "Backlight intensity", ["Low", "Medium", "Bright"], 1, 1),
  list(2002, "Power", "Backlight timeout", ["Off", "10 sec", "30 sec"], 2, 2),
  bool(2003, "Power", "Nightlight", false, false),
  bool(2004, "Power", "Buttons blink", true, true),
  float(1400, "Phones EQ treble", -0.0099, 0),
  float(1500, "Lineout EQ treble", 0.021, 0),
  float(300, "Lineout volume (mix)", 1.3456, 2.284794807434082),
];

// Switches keyed on the drive's name, like "broken" below: open a folder whose path contains
// "nosettings" (no settings.dat yet), "nofree" (free space unreadable) or "pending" (a firmware file
// already waits, so an install is refused) to see those states.
const has = (a: Record<string, unknown>, word: string) => String(a.root ?? "").toLowerCase().includes(word);

let running: { job: string; timers: ReturnType<typeof setTimeout>[]; what: string } | null = null;

/** One job-progress event per step on a timer, then job-done, like jobs.rs. */
function runSteps(job: string, labels: string[], status: string, summary: string, after?: () => void, detail?: (i: number, n: number) => string) {
  const timers: ReturnType<typeof setTimeout>[] = [];
  const what = job === "device-backup" ? "the backup was stopped and the partial copy was removed"
    : job === "device-firmware" ? "the firmware copy was stopped and nothing was installed"
    : "";
  running = { job, timers, what };
  labels.forEach((label, i) =>
    timers.push(setTimeout(
      () =>
        emit("job-progress", {
          job, label, done: i + 1, total: labels.length, status,
          bpm: null, detail: detail ? detail(i + 1, labels.length) : null, error: null,
        }),
      350 * (i + 1),
    )),
  );
  timers.push(setTimeout(() => {
    running = null;
    after?.();
    emit("job-done", { job, ok: labels.length, failed: 0, summary, error: null });
  }, 350 * (labels.length + 1)));
}

export const handlers: Handlers = {
  device_overview: (a) => ({
    storage: has(a, "nofree") ? { ...storage, total_bytes: 0, free_bytes: 0 } : storage,
    firmware: { ...firmware, pending_pfw: has(a, "pending") ? ["waiting-demo.pfw"] : [...firmware.pending_pfw] },
  }),

  device_health: (a) => {
    if (String(a.root).includes("broken")) {
      return {
        compatibility_problems: ["user_version is 12, device requires 677"],
        missing_audio: [],
        unreferenced_checked: false,
        unreferenced: [],
        unreferenced_bytes: 0,
        warnings: ["Unused files were not looked for: the library is not device-compatible, so its rows cannot be trusted."],
      };
    }
    return {
      compatibility_problems: [],
      missing_audio: [{ track_id: 7, location: "/pmdb_tracks/7/6/demo6.mp3" }],
      unreferenced_checked: true,
      unreferenced,
      unreferenced_bytes: unreferenced.reduce((n, f) => n + f.size, 0),
      warnings: [],
    };
  },

  device_settings: (a) => (has(a, "nosettings") ? null : { settings, other_items: 30, total_items: 104 }),

  device_settings_write: (a) => {
    let n = 0;
    for (const c of a.changes as { id: number; value: number }[]) {
      const s = settings.find((x) => x.id === c.id);
      if (!s || !s.editable) throw `setting ${c.id} cannot be edited`;
      if (s.value !== c.value) {
        s.value = c.value;
        s.changed = s.value !== s.default;
        n++;
      }
    }
    return n;
  },

  device_firmware_inspect: (a) => {
    const bad = String(a.path).toLowerCase().includes("bad");
    return {
      file_name: String(a.path).split(/[\\/]/).pop() || "firmware-demo.pfw",
      size: 50_000 * 1024 + 512,
      tag: "dfw_180096070",
      sw_version: "180096070",
      fw_version: "180096070",
      bootloader_version: "12838",
      md5_ok: !bad,
      problems: bad ? ["the checksum in the trailer does not match the file: it is damaged or incomplete"] : [],
      warnings: [],
    };
  },

  device_firmware_install: (a) => {
    if (has(a, "pending")) {
      throw "invalid: another firmware file (waiting-demo.pfw) is already waiting in .Pacemaker; remove it first so the device does not pick the wrong one";
    }
    runSteps("device-firmware", ["Copying the firmware file", "Copying the firmware file", "Copying the firmware file"], "copying",
      "Copied firmware-demo.pfw (48.8 MB) to .Pacemaker.", () => (firmware.pending_pfw = ["firmware-demo.pfw"]), (d, t) => `${d * 16} of ${t * 16} MB`);
  },

  device_backup: (a) => {
    const labels = [".Pacemaker/music.db", ".Pacemaker/settings.dat", ".Pacemaker/system.xml", ".Pacemaker/Music/0/1/demo1.str", ".Pacemaker/Music/2/3/demo2.str"];
    runSteps("device-backup", labels, "copied",
      `Backed up ${labels.length} files (4.0 MB) to ${a.dest}\\pacemaker-backup-20261111T101320Z. Every file was read back and checked; checksums are in MANIFEST.sha256.`);
  },

  device_cleanup: (a) => {
    const files = a.files as string[];
    runSteps("device-cleanup", files, "deleting", `Deleted ${files.length} files`, () => (unreferenced = unreferenced.filter((f) => !files.includes(f.path))));
  },

  device_eject: () => true,

  // stops the running copy like the backend: the work is undone and job-done carries the error
  device_cancel: () => {
    const r = running;
    if (!r) return;
    running = null;
    r.timers.forEach(clearTimeout);
    if (r.job === "device-cleanup") {
      emit("job-done", { job: r.job, ok: 0, failed: 0, summary: "Deleted 0 files, stopped early at your request; the rest were not touched", error: null });
    } else {
      emit("job-done", { job: r.job, ok: 0, failed: 0, summary: "", error: `invalid: Cancelled: ${r.what}` });
    }
  },
};
