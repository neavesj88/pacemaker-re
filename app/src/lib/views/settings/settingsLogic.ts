// Pure rules of the Settings view (run by plain node in `npm test`): which entries a search
// keeps, how they are grouped, and the checks that every area's list is well formed.
import { AREAS } from "./areas.ts";

export type Listed = { id: string; area: string; title: string; help: string; keywords?: string };

export type Group<T extends Listed> = { area: string; entries: T[] };

/** The words of a search: lower case, split on white space, empty ones dropped. */
export function words(query: string): string[] {
  return query.toLowerCase().split(/\s+/).filter(Boolean);
}

/** Whether every word of the search is found in the entry's title, help, keywords or area. */
export function matches(e: Listed, query: string): boolean {
  const ws = words(query);
  if (ws.length === 0) return true;
  const hay = `${e.title} ${e.help} ${e.keywords ?? ""} ${e.area}`.toLowerCase();
  return ws.every((w) => hay.includes(w));
}

/**
 * The entries a search keeps, grouped by area in `AREAS` order (an area with none left is not
 * listed), each group sorted by title. An entry with an area that is not in `AREAS` goes last
 * under its own name, so a typo hides nothing.
 */
export function group<T extends Listed>(entries: T[], query = ""): Group<T>[] {
  const kept = entries.filter((e) => matches(e, query));
  const known: string[] = [...AREAS];
  const extra = [...new Set(kept.map((e) => e.area))].filter((a) => !known.includes(a)).sort();
  return [...known, ...extra]
    .map((area) => ({ area, entries: kept.filter((e) => e.area === area).sort((a, b) => a.title.localeCompare(b.title)) }))
    .filter((g) => g.entries.length > 0);
}

/** Problems with a list of entries: a repeated id, an area that is not listed, a blank text. */
export function problems(entries: Listed[]): string[] {
  const out: string[] = [];
  const seen = new Set<string>();
  const known: string[] = [...AREAS];
  for (const e of entries) {
    if (!e.id || !/^[a-z0-9][a-z0-9-]*$/.test(e.id)) out.push(`"${e.id}" is not a usable id`);
    if (seen.has(e.id)) out.push(`"${e.id}" is used twice`);
    seen.add(e.id);
    if (!known.includes(e.area)) out.push(`${e.id}: "${e.area}" is not one of the areas`);
    if (!e.title.trim() || !e.help.trim()) out.push(`${e.id}: a title and a help sentence are needed`);
  }
  return out;
}
