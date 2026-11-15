"""Run: python scripts/test_restore_library.py  (no extra packages)."""
import hashlib
import importlib.util
import subprocess
import sys
import tempfile
from pathlib import Path

here = Path(__file__).parent
spec = importlib.util.spec_from_file_location("restore_library", here / "restore-library.py")
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)

# the app's manifest line (two spaces) and the sha256sum forms
assert mod.parse_line("ab12  .Pacemaker/music.db") == ("ab12", ".Pacemaker/music.db")
assert mod.parse_line("ab12 *./.Pacemaker/music.db") == ("ab12", ".Pacemaker/music.db")
assert mod.parse_line("ab12 .Pacemaker/music.db") == ("ab12", ".Pacemaker/music.db")

# end to end with a manifest exactly as the app writes it
with tempfile.TemporaryDirectory() as d:
    root = Path(d)
    backup, target = root / "backup", root / "target"
    files = {".Pacemaker/music.db": b"db", ".Pacemaker/settings.dat": b"set", ".Pacemaker/system.xml": b"xml", "Music/a.mp3": b"audio"}
    lines = []
    for rel, data in files.items():
        (backup / rel).parent.mkdir(parents=True, exist_ok=True)
        (backup / rel).write_bytes(data)
        lines.append(f"{hashlib.sha256(data).hexdigest()}  {rel}")
    manifest = root / "MANIFEST.sha256"
    manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")
    r = subprocess.run([sys.executable, str(here / "restore-library.py"), str(backup), str(manifest), str(target)], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr + r.stdout
    assert (target / ".Pacemaker/music.db").read_bytes() == b"db"
    assert (target / "Music/a.mp3").read_bytes() == b"audio"
    assert not (target / ".Pacemaker/settings.dat").exists() and not (target / ".Pacemaker/system.xml").exists()
    r = subprocess.run([sys.executable, str(here / "restore-library.py"), str(backup), str(manifest), str(target), "--settings-only"], capture_output=True, text=True)
    assert r.returncode == 0, r.stderr + r.stdout
    assert (target / ".Pacemaker/settings.dat").read_bytes() == b"set"
print("ok")
