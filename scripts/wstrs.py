"""UTF-32LE `strings`: JUCE on ARM Linux stores wchar_t literals as 4-byte code units."""
import re, sys
n = int(sys.argv[2]) if len(sys.argv) > 2 else 5
data = open(sys.argv[1], "rb").read()
for m in re.finditer(rb"(?:[\x20-\x7e]\x00\x00\x00){%d,}" % n, data):
    if m.start() % 4 == 0:
        print(f"{m.start():08x} {m.group().decode('utf-32-le')}")
