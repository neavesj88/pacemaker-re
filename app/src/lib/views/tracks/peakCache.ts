// Colour-waveform peaks for the Tracks view, fetched through the analysis contract
// (api/analysis.ts). Decoding audio is slow, so requests are queued, a few run at once, the most
// recently asked-for first (the rows the user just scrolled to), and a request nobody waits for
// any more is dropped before it starts. Results are kept in a small least-recently-used cache.
import { peaks as fetchPeaks, type Peak } from "$lib/api/analysis";

const MAX_CACHED = 1000;
const MAX_ACTIVE = 3;
const RETRY_AFTER_MS = 10_000;

type Done = (peaks: Peak[] | null) => void;
type Job = { key: string; root: string; location: string; width: number; waiters: Set<Done>; started: boolean; cancelled: boolean };

const cache = new Map<string, Peak[] | null>();
// A failed fetch is not cached as "no waveform"; it is only not retried for a few seconds so a
// broken backend is not hammered by every redraw.
const failed = new Map<string, number>();
const FAILED_KEPT = 2000;
const jobs = new Map<string, Job>();
const queue: Job[] = [];
let active = 0;

function remember(key: string, value: Peak[] | null) {
  cache.delete(key);
  cache.set(key, value);
  if (cache.size > MAX_CACHED) cache.delete(cache.keys().next().value!);
}

function pump() {
  while (active < MAX_ACTIVE) {
    const job = queue.pop(); // newest first
    if (!job) return;
    // withdrawn while it waited: skipped here instead of searched for in the queue when it was
    // withdrawn (a case of thousands of rows withdraws them all when the view closes)
    if (job.cancelled) continue;
    job.started = true;
    active++;
    fetchPeaks(job.root, job.location, job.width)
      .then((p): Peak[] | null => {
        failed.delete(job.key);
        return p.length ? p : null;
      })
      .catch((): Peak[] | null => {
        failed.set(job.key, Date.now());
        // oldest first out, so a library full of unreadable files cannot grow the map for ever
        if (failed.size > FAILED_KEPT) failed.delete(failed.keys().next().value!);
        return null;
      })
      .then((p) => {
        if (!failed.has(job.key)) remember(job.key, p);
        jobs.delete(job.key);
        active--;
        for (const done of job.waiters) done(p);
        pump();
      });
  }
}

/** The cached peaks for this request: the array, null (known to have none) or undefined (not
 *  fetched yet). Lets a row that scrolls back into view paint without waiting. */
export function cachedPeaks(root: string, location: string, width: number): Peak[] | null | undefined {
  return cache.get(`${root}\n${location}\n${width}`);
}

/** Peaks of the track at `location` resampled to `width` columns, delivered to `done` (at once
 *  when cached; null when the track has no waveform). Returns a function that withdraws the
 *  request, for when the row scrolls away or the view closes. */
export function requestPeaks(root: string, location: string, width: number, done: Done): () => void {
  const key = `${root}\n${location}\n${width}`;
  if (cache.has(key)) {
    const hit = cache.get(key)!;
    remember(key, hit);
    done(hit);
    return () => {};
  }
  const failedAt = failed.get(key);
  if (failedAt !== undefined) {
    if (Date.now() - failedAt < RETRY_AFTER_MS) {
      done(null);
      return () => {};
    }
    failed.delete(key);
  }
  let job = jobs.get(key);
  if (!job) {
    job = { key, root, location, width, waiters: new Set(), started: false, cancelled: false };
    jobs.set(key, job);
    queue.push(job);
  }
  job.waiters.add(done);
  const mine = job;
  pump();
  return () => {
    mine.waiters.delete(done);
    if (!mine.started && mine.waiters.size === 0) {
      mine.cancelled = true;
      jobs.delete(key);
    }
  };
}

/** Drops what is remembered as "no waveform" (an empty answer, or a failed fetch not yet retried)
 *  for these tracks of `root`, because the background analysis has just made them. Returns the
 *  locations that had such an entry: only their rows need to ask again. */
export function forgetPeaks(root: string, locations: string[]): string[] {
  const wanted = new Set(locations);
  const hit = new Set<string>();
  const parse = (key: string) => {
    const [r, location] = key.split("\n");
    return r === root && wanted.has(location) ? location : null;
  };
  for (const [key, value] of [...cache]) {
    const loc = parse(key);
    if (loc !== null && value === null) {
      cache.delete(key);
      hit.add(loc);
    }
  }
  for (const key of [...failed.keys()]) {
    const loc = parse(key);
    if (loc !== null) {
      failed.delete(key);
      hit.add(loc);
    }
  }
  return [...hit];
}
