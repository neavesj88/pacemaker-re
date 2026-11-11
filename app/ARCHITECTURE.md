# Pacemaker Revival: app architecture

The app is split so that many feature agents can work at once, each inside its own files. This
document is the contract. A file is **owned** by one area (only that area edits it) or **shared**
(changed only in the integration step, never by a feature agent in parallel).

Areas, each with its own folder / files in every layer: `tracks`, `mixes`, `cases`, `history`,
`grid`, `device`, `import`, `playback`, and the feature-only areas `metadata` (tag and rating
editor), `analysis` (key detection and other analysis jobs), `export` (playlists and library
listings out) and `stems` (vocal / instrumental separation into a PC folder). `library` is the session plumbing (`commands/library.rs`, `api/library.ts`,
`devmock/library.ts`, the `LibraryView` payload) and belongs to **integration**: it is not a
feature area. "Integration" means the single step that merges feature work and may touch shared
files.

## File map

### Frontend (`app/src`)

| Path | Purpose | Owner |
| --- | --- | --- |
| `routes/+page.svelte` | Thin shell: header, banners, job strip, sidebar, active view, player slot, overlays | integration |
| `routes/+layout.svelte`, `+layout.ts` | Global CSS; SPA mode; installs devmock outside Tauri | integration |
| `lib/styles/tokens.css`, `base.css` | The device palette (do not change colours); element defaults, `.muted` / `.num` / `.empty` | integration |
| `lib/format.ts` | `mmss`, `baseName`, `volumeName` | integration |
| `lib/shell/Header`, `Sidebar`, `Banners`, `Welcome`, `JobStrip` (`.svelte`) | App chrome; `JobStrip` shows any job | integration |
| `lib/shell/overlays.ts`, `startup.ts` | Auto-discover `views/*/Overlays.svelte` and `views/*/init.ts` (see below) | integration |
| `lib/views/registry.ts`, `ids.ts` | Auto-discovers `views/*/view.ts`; `ViewDef` type; `ViewId` is a string | integration |
| `lib/views/<area>/view.ts` | One row for the area's sidebar entry (id, label, section, order, component, count) | that area |
| `lib/views/tracks/` (`TracksView`, `TrackTable`, `trackFilter.ts`, `NeedsAnalysis`) | Tracks view, the reusable track table, the sidebar "need analysis" block | tracks |
| `lib/views/mixes/` (`MixesView`, `MixEditor`, `MixList`, `Timeline`, `flow.ts`, `preview.svelte.ts`, `init.ts`, ...) | Mix list and editor; plays previews through the shared player (`playback.previewMix`); follows the `mix-export` job in `init.ts`. Backend: `commands/mixes.rs` plus `mixes/{doc,timing,info}.rs`; `lib/api/mixes.ts`; store `lib/state/mixes.svelte.ts` (`mixes`); pure edit logic `lib/state/mixedit.ts`; tests `app/scripts/test-mixedit.mjs` (`npm run test:mixedit`, Node >= 22.6). Audio engine: `crates/pm-mixer`, commands in `commands/export.rs` | mixes |
| `lib/views/cases/` (`CasesView`, `SmartCases`) | Cases; the sidebar "Smart cases" section | cases |
| `lib/views/stems/` (`StemsView`, `SetupCard`, `MakeCard`, `SettingsCard`, `PlanModal`, `LicenceModal`, `ResultCard`, `logic.ts`, `init.ts`) | Stems view: vocal and instrumental files for chosen tracks into a folder on this computer; model download behind a licence notice. Store `lib/state/stems.svelte.ts` (`stems`); `lib/api/stems.ts`; `lib/devmock/stems.ts`; tests `logic.test.mjs` (in `npm test`). Engine: `crates/pm-stems` (provider trait, STFT, overlap-add, HTDemucs via ONNX Runtime, file writers) | stems |
| `lib/views/history/`, `grid/`, `device/` | History, beat grid editor, device views (placeholders) | that area |
| `lib/views/import/` (`ImportView`, `ConfirmModal`, `DropOverlay`, `Overlays.svelte`, `init.ts`) | Import view; write confirmation and drop overlay; job-event and file-drop subscriptions | import |
| `lib/views/<area>/Overlays.svelte`, `init.ts` | Optional: always-mounted overlays / startup subscriptions of that area | that area |
| `lib/player/Player.svelte` (+ optional `Overlays.svelte`, `init.ts`) | Bottom player bar and engine | playback |
| `lib/ui/*` | Generic primitives: `Button`, `Modal`, `Table`, `CollectionTable`, `Stars`, `Waveform`, `ContextMenu`, `Toast`, `Placeholder`, `RMark` | integration |
| `lib/state/session.svelte.ts` | Detected volumes, the open library, load / reload, error banner text, `onReset` hooks | integration |
| `lib/state/nav.svelte.ts` | Current view and the one-shot deep-link param bag | integration |
| `lib/state/filters.svelte.ts`, `selection.svelte.ts`, `toast.svelte.ts` | Header search / needs filter / sort; highlighted track ids; toasts | integration |
| `lib/state/jobs.svelte.ts` | Generic job runner: `jobs.ask(request)`, progress, completion; add / re-analyse adapters | import |
| `lib/state/cases.svelte.ts` | The open case and its track ids | cases |
| `lib/state/playback.svelte.ts` | The transport (loaded track, playing, position, mix preview). Others call its methods only | playback |
| `lib/state/<area>.svelte.ts` | An area's own store, created by that area when needed | that area |
| `lib/api/types.ts`, `events.ts` | Wire types shared across areas (Track, Collection, LibraryView, job events); event names, `subscribeAll` | integration |
| `lib/api/<area>.ts` | One typed `invoke()` wrapper module per area (`library`, `tracks`, `cases`, `import`, `mixes`, `history`, `grid`, `device`, `playback`, `metadata`, `analysis`, `export`) | that area |
| `lib/devmock/index.ts` | Merges every `devmock/<area>.ts` automatically | integration |
| `lib/devmock/db.ts`, `jobs.ts`, `dialog.ts`, `types.ts` | Shared mock fixture (tracks, cases), job runner, dialog mock | integration |
| `lib/devmock/<area>.ts` | That area's mock command handlers and its own fixtures (the mix fixture is in `devmock/mixes.ts`) | that area |

### Backend (`app/src-tauri/src`)

| Path | Purpose | Owner |
| --- | --- | --- |
| `lib.rs` | Registration only: plugins, `setup`, `register_handlers!()` | integration |
| `commands/mod.rs` | Module list, `setup` chain, `register_handlers!` chain (all twelve areas are already in it) | integration |
| `commands/library.rs` | `list_volumes`, `open_library`, `LibraryView` | integration |
| `commands/<area>.rs` | Commands of that area: `tracks` (`reanalyse_tracks`), `mixes` (`mix_list`), `cases` (`case_tracks`), `import` (`expand_audio_paths`, `add_tracks`); `history`, `grid`, `device`, `playback`, `metadata`, `analysis`, `export` are empty | that area |
| `state.rs` | State shared by several areas (`JobLock`); `setup` | integration |
| `jobs.rs` | Generic `spawn_job`, `Progress`, `JobOutcome`, `JobDone` | integration |
| `events.rs` | Shared event names (jobs) and `emit` | integration |
| `error.rs` | `AppError` (serialises to a string), `AppResult`, `.app()` | integration |

`commands/stems.rs` plus `commands/stems/{manifest,download,settings,plan,run}.rs` are the `stems` area: consented downloads of the model and engine into the app data folder, settings and consent records in the app config folder, a plan that the run recomputes with the same function, and a job that writes only into a folder on this computer (the card is reached through the ordinary add-tracks step, which asks first).

Crates in `crates/` are not part of the app layout; the app only calls them.

## Rules of ownership

1. Edit only files your area owns, plus new files you create inside your own folders.
2. A shared file needs a change only when you cross an area boundary; write the request in your
   hand-off note and let the integration step make it. Never edit a shared file in parallel.
3. A view may import another view's *exported component* only where listed here (the Cases view
   uses `views/tracks/TrackTable.svelte` and `trackFilter.ts`). Everything else goes through
   `lib/state` and `lib/api`.
4. Use the CSS variables in `tokens.css`; do not write raw colours. Keep the R mark and palette.
5. Never put real track, artist, case or mix names, or serial numbers, in code, mocks or tests.
6. **No dependencies mid-run.** `Cargo.lock` and `package-lock.json` are single files; parallel
   `cargo add` / `npm install` conflict. If you need a crate, package, Tauri feature or
   permission, write it in your hand-off note; the integration step adds it. Already decided:
   audio is streamed to an `<audio>` element over the asset protocol (`protocol-asset` is on,
   scope empty; playback adds the open library's files with
   `app.asset_protocol_scope().allow_file(..)` at run time); waveform peaks come from
   `pm-analysis` decode in Rust; tag and rating edits go through the `pm-formats` `Library` (no
   tag-writing crate); exports are plain text formats (M3U, CSV) written with `std::fs`.
7. **Public surfaces others depend on.** `TrackTable` props and `trackFilter.ts` exports are
   frozen for existing callers: tracks may add props only with defaults that keep current
   behaviour. `ui/Waveform.svelte` is integration-owned: an area that needs more (zoom, cues,
   editable grid lines) wraps it or copies it into its own folder (e.g. `views/grid/`), it does
   not edit it. The `Track` and `LibraryView` payloads are frozen: new per-track data comes from
   an area command (`<area>_track_detail(root, id)`, or a list command of your own like
   `mix_list`), never by widening `Track`. A field that truly must be added is an integration
   request.

## How to add things

### A view's content (your area already has a folder)

Edit the `<Area>View.svelte` in your folder. Split it into more components in the same folder as
it grows. The registry already shows it in the sidebar.

### A new top-level view

Create `lib/views/<area>/` with `<Area>View.svelte` and a `view.ts` (copy `views/mixes/view.ts`:
a default-exported `ViewDef` with `id`, `label`, `section`, `order`, `component`, optional
`count`). `registry.ts` and `ids.ts` are not edited. Navigate with `nav.go("<id>")`.

### A modal, drop zone or other always-mounted UI

Add `lib/views/<area>/Overlays.svelte` (or `lib/player/Overlays.svelte`). The shell renders every
such file once. Nothing shared is edited.

### A startup subscription (events, OS listeners)

Add `lib/views/<area>/init.ts` (or `lib/player/init.ts`) with
`export default function init(): Unlisten` (use `subscribeAll` from `lib/api/events.ts`). The
shell runs every such file when it mounts and calls the returned teardown on unmount.

### Opening a view with a target

`nav.go("grid", { "grid.trackId": 7 })`; the receiving view reads `nav.take<number>("grid.trackId")`
(removes it so it is acted on once). Prefix keys with the area name.

### A store

Create `lib/state/<area>.svelte.ts`. Use a class with `$state` fields and export one instance. If
it holds per-library state, register `session.onReset(() => store.clear())` at module level
(see `cases.svelte.ts`); `session.svelte.ts` is never edited.

```ts
class Mixes {
  openId = $state<number | null>(null);
  open(id: number) { this.openId = id; }
}
export const mixes = new Mixes();
```

Read library data from `session.library`; do not copy it into your store. Stores must not import
views. A store may import `lib/api/<area>.ts` and other stores, never a cycle.

### An API call

In `lib/api/<area>.ts`:

```ts
export const getHistory = (root: string) => invoke<Session[]>("history_sessions", { root });
```

Command names are the Rust function names; argument keys are camelCase (`track_ids` is `trackIds`).
Types used by one area stay in its api file; types several areas need go in `api/types.ts`
(integration step). Errors arrive as strings; `catch (e) { session.error = String(e) }`.

### A Tauri command

In `src-tauri/src/commands/<area>.rs`:

1. Write `#[tauri::command] pub async fn name(...) -> AppResult<T>`. Do blocking work in
   `tauri::async_runtime::spawn_blocking`. Open the library with `Library::open_read_only` to read,
   `Library::open` only for a command that writes, and write only after an explicit user action.
2. Add `$crate::commands::<area>::name,` to the list in that file's `<area>_commands!` macro.
   **This step is required**: a command that is not listed compiles but fails at run time with
   "command not found".
3. Add the wrapper in `lib/api/<area>.ts` and the mock in `lib/devmock/<area>.ts`.

`lib.rs` and `commands/mod.rs` are not touched. Errors: `?` works on `pm_formats::library::Error`,
`std::io::Error` and `tauri::Error`; for any other `Display` error use `result.app()?`
(`use crate::error::IntoApp`). Managed state used by only your area: `app.manage(MyState::default())`
in your file's `setup`. State shared by several areas goes in `state.rs` (integration step).

### A background job (any kind: key detection, export, sync, tag write)

Backend: call `jobs::spawn_job(app, &lock, "my-kind", |app| { ...; Ok(JobOutcome { ok, failed, summary }) })`
from a command that returns once the job has started. Emit per-item progress with
`events::emit(app, events::JOB_PROGRESS, Progress::new("my-kind", label, done, total, "my-status"))`
(set `.bpm`, `.detail` or `.error` as needed; status `"failed"` lists the item as failed). The
`summary` string is shown as is when the job finishes. `spawn_job` holds the shared `JobLock`, so
only one job runs at a time. Never reuse another area's kind string.

Frontend: nothing shared is edited. From your store or view call
`jobs.ask({ kind, root, count, names, title, description, confirmLabel, runningLabel, doneLabel, start })`
where `start` invokes your command. The confirmation, progress strip, failure list, completion and
library reload are handled by `state/jobs.svelte.ts`, `shell/JobStrip.svelte` and
`views/import/ConfirmModal.svelte`. Call `jobs.guard()` first if you need to check that writing is
possible before doing work. In devmock use `runJob(kind, labels, each)` from `devmock/jobs.ts`.

### An event

1. Backend: add `pub const MY_EVENT: &str = "my-event";` **to your own `commands/<area>.rs`** and
   emit with `events::emit(&app, MY_EVENT, payload)` (payload: `Serialize + Clone`). Only events
   that several areas listen to belong in `events.rs`.
2. Frontend: in `lib/api/<area>.ts`, export a typed wrapper
   `export const onMyEvent = (cb) => listen<Payload>("my-event", (e) => cb(e.payload))` (the event
   name string is declared once, there), and register it in your `init.ts` with
   `subscribeAll([...])` (it returns one teardown function and tolerates running in a plain browser).
3. In devmock, emit it with `emit("my-event", payload)` from `@tauri-apps/api/event`.

### A devmock entry

In `lib/devmock/<area>.ts` add to the exported `handlers` object, keyed by command name:

```ts
export const handlers: Handlers = {
  history_sessions: () => [],   // the same shape the Rust command returns; throw a string to fail
};
```

The file is merged by `devmock/index.ts` automatically (a command name defined twice throws at
start). Fixtures private to your area live in your file; `devmock/db.ts` holds only the shared
track and case fixture. The mock is invented placeholder data only.

## Writes to the device

Every change to the device volume goes through `state::WriteGate`, so that Eject, backup and
firmware install never run under a write, quick writes never fail because a long job is running,
and `music.db` always has a safety copy from before the session's first change.

| Kind | Examples | How |
| --- | --- | --- |
| Quick write | rating, tags, cases, smart-case device copy, mix new / save / rename / delete / import, history "save as case", grid save, `settings.dat`, the Vocal filter flag file (`.Pacemaker/revival-vocal`, `device_vocal_set`) | `state::quick_write(app, root, \|\| ...)` on a blocking thread |
| Write job | add tracks, re-analyse, key detection, library import | `jobs::spawn_write_job(app, &lock, kind, root, \|app\| ...)`: `JobLock` (one job at a time) plus a write ticket for the body |
| Exclusive operation | Eject, backup, firmware install | `state::take_exclusive` (backup, firmware: a job, so `spawn_job` holds the `JobLock`) or `state::take_exclusive_locked` (Eject: takes the `JobLock` itself), held until the operation ends |
| Device-deleting job | Device clean-up | `spawn_job` plus a `WriteTicket` inside the job, so it is refused during an eject / backup and gets the session snapshot |
| Not a device write | mix audio export, M3U / CSV out, preview render | `spawn_job` with the `JobLock` only |

Rules:

1. A quick write and the body of a write job each hold a `WriteTicket`. Any number of tickets can
   exist at once; SQLite's own file lock orders the statements, and `Library::open` sets a 10 s
   busy timeout, so a rating saved while "add tracks" runs waits for the job's current insert
   instead of failing with "database is locked". A quick write is never refused because of
   `JobLock`.
2. An exclusive operation sets a flag first (new writes are refused at once with a clear message),
   then waits up to 5 s for tickets already out to be returned, then runs. If a write job holds
   `JobLock`, Eject and the others refuse immediately ("a job is still running"); if a quick write
   is stuck past 5 s they give up and release the gate. Eject also holds the `JobLock` until it
   returns, so no job (clean-up, export, add) can start under it. Backup is exclusive so that
   `music.db` and the `.str` files are not copied mid-write: for the whole backup (minutes with
   audio included) every quick write is refused with "the device is being backed up", not
   queued. That is deliberate (a rating saved mid-copy would leave the backup describing a state
   the device never had); the message names the operation so the user knows to wait.
3. `WriteGate::begin_write` also makes the safety copy: before the first write of the session to a
   library (keyed by volume root) it copies `music.db`, `music.db-journal` if present and
   `settings.dat` to `<app local data>/snapshots/<volume>/music-db-<UTC stamp>/` (never onto the
   device), read back and checksummed, and keeps the newest 10 per library
   (`pm_formats::ops_device::snapshot_library`; the volume key ignores a trailing separator and, on
   Windows, case, so `E:` and `E:\` are one library, and the folder name carries a hash of the
   key so `/mnt/a-b` and `/mnt/a_b` never share one). If the copy fails the write is refused with the
   reason, and the next attempt tries again. A successful Eject forgets the volume, so a different
   device that appears under the same drive letter is snapshotted on its first write. The Device
   view's Backup card lists the folder (`device_snapshots`, `device_snapshots_open`). Restoring is
   a manual copy of the files back into `.Pacemaker`.
4. Files are written temp + fsync + rename (`library::atomic_write*`, `install_pfw`'s `.part`,
   backup's `.partial` folder); every failure path removes the temp file, so an unplug leaves at
   worst a `*.tmp` (the Device clean-up lists those under `/pmdb_tracks`; the next write of the
   same file removes its own temps older than 10 minutes, which covers `.Pacemaker`), never a
   truncated file under a real name. `install_pfw` renames over an existing `.pfw` without
   deleting it first. A `.str` rewrite (`store_analysis`) restores the old file if the row update
   then fails, so file and row never describe different grids.
5. A new write command: use `quick_write` or `spawn_write_job`, never `Library::open` directly in
   a command. A new exclusive operation: take `take_exclusive` before it starts.

The Vocal filter (Revival firmware 180096073: the Wah tool becomes a vocal filter) is one flag file on the card.
`pm_formats::ops_device::{read_vocal, write_vocal}` own its fixed path; `revival-no-preload` (kill switch) is only ever read.
Mix previews and exports ask `export/render.rs::peq_mode(root)` for the same effective mode, so what is previewed is what is
exported; the mix editor names the Wah tool "Vocal" from the same flag (`state/vocal.svelte.ts`).

Mix audio export is cancellable (`export_cancel` sets a flag the render's progress callback turns
into `false`; the engine removes its `.partial` file). Saving a beat grid for a track that mixes
use asks first (`grid_mixes_using` names them); the clips keep the old tempo until re-beatmatched.

## Shared and therefore integration-only

`routes/*`, `lib/styles/*`, `lib/format.ts`, `lib/shell/*`, `lib/ui/*`, `lib/views/ids.ts`,
`lib/views/registry.ts`, `lib/state/{session,nav,filters,selection,toast}.svelte.ts`,
`lib/api/{types,events}.ts`, `lib/devmock/{index,db,jobs,dialog,types}.ts`, `src-tauri/src/lib.rs`,
`commands/{mod,library}.rs`, `state.rs`, `jobs.rs`, `events.rs`, `error.rs`, `Cargo.toml`,
`package.json`, `tauri.conf.json`, `capabilities/*`.

Adding a dependency or a Tauri permission is also integration-only (rule 6).

## Checks

In `app/`: `npm run check` and `npm run build`. In `app/src-tauri/`: `cargo build` and
`cargo clippy --all-targets`. From the repo root: `cargo test -p pm-formats -p pm-analysis`.
`npm run dev` serves the UI in a browser against devmock (no Tauri needed). `svelte-check` covers
every file, so a half-finished component in one area shows up in everyone's check: keep your area
compiling, or stub the part you have not finished.
