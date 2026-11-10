// Browser preview: when the UI runs outside Tauri (plain `npm run dev` in a browser), answer
// the app's commands with a synthetic library so the interface can be developed and checked
// without the native shell. Never loaded inside the Tauri app. Nothing here touches a disk.
//
// Every devmock/<area>.ts that exports `handlers` is merged in automatically (no edit here to
// add an area). A command name defined twice is a bug and throws at startup.
import { mockIPC } from "@tauri-apps/api/mocks";
import type { Handler, Handlers } from "./types";

const modules = import.meta.glob<{ handlers?: Handlers }>(["./*.ts", "!./index.ts"], { eager: true });

const all = new Map<string, Handler>();
for (const [file, mod] of Object.entries(modules)) {
  for (const [cmd, fn] of Object.entries(mod.handlers ?? {})) {
    if (all.has(cmd)) throw new Error(`devmock: command ${cmd} is defined twice (second in ${file})`);
    all.set(cmd, fn);
  }
}

export function installDevMock() {
  mockIPC(
    (cmd, args) => {
      const fn = all.get(cmd);
      if (!fn) throw new Error(`devmock: unhandled command ${cmd}`);
      return fn((args ?? {}) as Record<string, any>);
    },
    { shouldMockEvents: true },
  );
}
