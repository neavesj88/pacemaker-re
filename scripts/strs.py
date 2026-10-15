"""Minimal `strings` (Git Bash lacks binutils): printable ASCII runs >= N, with hex offsets."""
import re, sys
n = int(sys.argv[2]) if len(sys.argv) > 2 else 6
data = open(sys.argv[1], "rb").read()
for m in re.finditer(rb"[\x20-\x7e]{%d,}" % n, data):
    print(f"{m.start():08x} {m.group().decode()}")
