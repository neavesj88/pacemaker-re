# Pacemaker Revival (app)

Desktop app for the Pacemaker: Tauri 2 shell (`src-tauri/`) over the workspace crates, with a
Svelte 5 + TypeScript UI (`src/`).

    npm install
    npm run tauri dev                       # native app with hot reload
    npx tauri build --no-bundle             # release binary in src-tauri/target/release

`npm run dev` alone serves the UI in a browser with a synthetic library (`src/lib/devmock.ts`),
for UI work without the native shell.

Current scope: find a docked Pacemaker (or open a folder holding `.Pacemaker`), check the
database against the device's rules, and browse tracks, mixes and cases. Browsing is read-only.
