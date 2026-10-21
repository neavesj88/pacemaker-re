# Pacemaker Revival (app)

Desktop app for the Pacemaker: Tauri 2 shell (`src-tauri/`) over the workspace crates, with a
Svelte 5 + TypeScript UI (`src/`).

    npm install
    npm run tauri dev                       # native app with hot reload
    npx tauri build --no-bundle             # release binary in src-tauri/target/release

`npm run dev` alone serves the UI in a browser with a synthetic library (`src/lib/devmock.ts`),
for UI work without the native shell.

Current scope: find a docked Pacemaker (or open a folder holding `.Pacemaker`), check the
database against the device's rules, and browse tracks, mixes and cases (a case lists its
tracks). Browsing is read-only; only two explicit actions write to the volume, each after a
confirmation naming the destination:

- **Add tracks…** (file picker, or drop audio files or folders on the window): each file is
  analysed, copied to `.Pacemaker/Music/…`, and added with its `.str`. Tracks already on the
  volume are skipped without re-analysing; one bad file doesn't stop the batch.
- **Analyse N…**: tracks whose `.str` is missing, empty or unreadable are marked "needs
  analysis"; this rewrites just their `.str` and BPM from the audio already on the volume.

The shared logic is `pm_analysis::ingest`; `src/lib/devmock.ts` mirrors the commands for the
browser preview.
