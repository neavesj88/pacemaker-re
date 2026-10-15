# pacemaker-re

Reverse-engineering notes and tooling toward an open-source replacement for the Tonium Pacemaker
Editor (last release 2.0.2.14170). Goal: current Pacemaker owners can manage their device from
modern Windows, Linux and macOS.

Start with [notes/FINDINGS.md](notes/FINDINGS.md). The recovered SQLite schema is
[notes/schema-14142.sql](notes/schema-14142.sql).

## Reproduce

    scripts/fetch.sh              # downloads originals into vendor/ and writes SHA256SUMS
    scripts/unpack-firmware.sh    # ext2 firmware images -> work/fw-<ver>/
    python -I scripts/isx.py "vendor/editor-win/Pacemaker Setup.exe" work/editor-win-setup
    python -I scripts/gen-schema.py work/fw-14142/bin/pmplayer > notes/schema-14142.sql

These need 7-Zip (`SEVENZIP` overrides the default Windows path) and Python 3. `scripts/rea.sh`
wraps the [rea](https://github.com/morluto/rea) CLI.

## What is not in this repo

`vendor/`, `work/` and `tools/` are gitignored. Tonium's firmware, Editor and restore tools are
copyrighted. This repo links to them (see `scripts/fetch.sh`) and never redistributes them. The
project relies on reverse engineering for interoperability.
