The app ships the newest Pacemaker Revival firmware from this folder (a Tauri resource).
`npm run bundle-firmware` (run by the release build) copies the newest
work/build/pacemaker-revival-<number>.pfw here. The .pfw is not committed.
This file only makes sure the resource glob always matches.
