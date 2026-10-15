"""UTF-16LE `strings` for Windows binaries (wchar_t is 2 bytes there)."""
import re, sys
n = int(sys.argv[2]) if len(sys.argv) > 2 else 5
data = open(sys.argv[1], "rb").read()
for m in re.finditer(rb"(?:[\x20-\x7e]\x00){%d,}" % n, data):
    print(f"{m.start():08x} {m.group().decode('utf-16-le')}")
