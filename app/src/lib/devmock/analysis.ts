// Mock of src-tauri/src/commands/analysis.rs. Add one entry per command, keyed by command name;
// this file is picked up automatically by devmock/index.ts.
import type { Handlers } from "./types";

function seeded(text: string) {
  let s = 0;
  for (const c of text) s = (s * 31 + c.charCodeAt(0)) >>> 0;
  return () => ((s = (s * 1103515245 + 12345) >>> 0) / 2 ** 32);
}

export const handlers: Handlers = {
  // synthetic but stable per track: a kick-heavy body with quieter intro and outro
  analysis_peaks: (a) => {
    const r = seeded(String(a.location));
    const width = Math.max(1, Math.min(4096, Number(a.width) || 100));
    return Array.from({ length: width }, (_, i) => {
      const t = i / width;
      const env = t < 0.08 || t > 0.92 ? 0.35 : 0.8 + 0.2 * Math.sin(t * 9);
      return [Math.round(255 * env * (0.55 + 0.45 * r())), Math.round(170 * env * r()), Math.round(110 * env * r())];
    });
  },
};
