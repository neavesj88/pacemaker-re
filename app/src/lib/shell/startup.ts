// App-lifetime startup hooks. Every `views/<area>/init.ts` (and `player/init.ts`) that default-
// exports `() => Unlisten` is run once when the shell mounts; the returned teardown runs when it
// unmounts. An area adds its file; nothing here is edited.
import type { Unlisten } from "$lib/api/events";

const modules = import.meta.glob<{ default: () => Unlisten | void }>(["../views/*/init.ts", "../player/init.ts"], {
  eager: true,
});

export function startAll(): Unlisten {
  const stops = Object.values(modules).map((m) => m.default());
  return () => stops.forEach((s) => s?.());
}
