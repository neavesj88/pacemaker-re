"""Copy a backed-up Pacemaker library onto a freshly restored drive, checking every file.

The backup is a copy of the device's FAT32 volume plus a manifest with one `<hash>  <relative/path>`
line per file. The Device page's Backup writes it inside the backup folder as MANIFEST.sha256; the
`sha256sum` form `<hash> *./path` also works. Each file is copied, read back and hashed; files already
present with the right hash are skipped, so the script can be re-run after an interruption.

  restore-library.py BACKUP_DIR MANIFEST TARGET_ROOT [--pfw FILE] [--settings-only]

By default everything except `.Pacemaker/settings.dat` and `.Pacemaker/system.xml` is copied:
the device deletes settings.dat after installing a firmware image and rewrites system.xml on every
boot. `--pfw` also copies a firmware image into `.Pacemaker/` for the device to install.
`--settings-only` copies just settings.dat, for after that install.
"""
import argparse
import hashlib
import shutil
import sys
from pathlib import Path

SKIPPED = {".Pacemaker/settings.dat", ".Pacemaker/system.xml"}


def sha256(path: Path) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def parse_line(line: str) -> tuple[str, str]:
    """One manifest line: `<hash>  <path>` (the app, two spaces) or `<hash> *./<path>` (sha256sum)."""
    digest, rel = line.split(" ", 1)
    return digest, rel.lstrip(" *").removeprefix("./")


def put(src: Path, dst: Path, want: str) -> str:
    if dst.exists() and dst.stat().st_size == src.stat().st_size and sha256(dst) == want:
        return "kept"
    dst.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(src, dst)
    got = sha256(dst)
    if got != want:
        sys.exit(f"MISMATCH after copy: {dst} ({got} != {want})")
    return "copied"


def main() -> None:
    ap = argparse.ArgumentParser()
    ap.add_argument("backup", type=Path)
    ap.add_argument("manifest", type=Path)
    ap.add_argument("target", type=Path)
    ap.add_argument("--pfw", type=Path)
    ap.add_argument("--settings-only", action="store_true")
    a = ap.parse_args()

    entries = []
    for line in a.manifest.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        digest, rel = parse_line(line)
        wanted = rel == ".Pacemaker/settings.dat" if a.settings_only else rel not in SKIPPED
        if wanted:
            entries.append((digest, rel))
    if a.settings_only and not entries:
        sys.exit("settings.dat is not in the manifest")

    counts = {"copied": 0, "kept": 0}
    for i, (digest, rel) in enumerate(entries, 1):
        src = a.backup / rel
        if sha256(src) != digest:
            sys.exit(f"backup file does not match its manifest entry: {src}")
        counts[put(src, a.target / rel, digest)] += 1
        print(f"\r{i}/{len(entries)}", end="", flush=True)
    print()
    if a.pfw:
        counts[put(a.pfw, a.target / ".Pacemaker" / a.pfw.name, sha256(a.pfw))] += 1
        print(f"firmware image: {a.pfw.name}")
    print(f"{counts['copied']} copied, {counts['kept']} already present, all hashes match")


if __name__ == "__main__":
    main()
