// Waveforms the mock library does not have yet, for the background analysis preview. The real
// `analysis_peaks` makes a missing waveform on demand; the mock instead answers an empty list for
// these tracks until the simulated background worker (devmock/bganalysis.ts) has "made" them, so
// the table can be seen filling in. Track 7 is one the worker cannot decode: it stays empty and
// is listed as "couldn't be analysed".
import { tracks } from "./db";

export const heldPeaks = new Set<string>(tracks.filter((t) => t.track_id % 2 === 0 || t.track_id === 7).map((t) => t.location));
export const UNREADABLE_TRACK = 7;
