"""Dump every {column, SQL type} pointer-pair array in an ELF32-LE binary.

The Pacemaker code builds CREATE TABLE statements from static arrays of (name, type) string
pointers, so each run of such pairs is one table. Anything pointing at a run's first pair is
printed too: that is usually the table descriptor holding the table name.
"""
import re, struct, sys
sys.path.insert(0, __import__("os").path.dirname(__file__))
from ptrtables import load, cstr, o2v

TYPE = re.compile(r"^(INTEGER|REAL|TEXT|BLOB|NUMBER\(\d+\)|VARCHAR\(\d+\)|INTEGER PRIMARY KEY.*)( DEFAULT \S+)?( NOT NULL)?$")

def main(path):
    d, segs = load(path)
    for va0, off, sz in segs:
        o, end = off, off + sz - 8
        while o < end:
            run = []
            p = o
            while p < end:
                a, b = struct.unpack_from("<II", d, p)
                n, t = cstr(d, segs, a), cstr(d, segs, b)
                if n and t and TYPE.match(t) and re.match(r"^[a-z_][a-z0-9_.]*$", n):
                    run.append((n, t)); p += 8
                else:
                    break
            if len(run) >= 2:
                start_va = o2v(segs, o)
                refs = []
                needle = struct.pack("<I", start_va); i = d.find(needle)
                while i != -1:
                    ctx = [cstr(d, segs, struct.unpack_from("<I", d, i + 4 * k)[0]) for k in range(-3, 4)]
                    refs.append(f"0x{i:x}: {[c for c in ctx if c]}"); i = d.find(needle, i + 4)
                print(f"\n# table at file 0x{o:x} va 0x{start_va:x} ({len(run)} cols) refs={refs}")
                for n, t in run:
                    print(f"  {n:22} {t}")
                o = p
            else:
                o += 4

if __name__ == "__main__":
    main(sys.argv[1])
