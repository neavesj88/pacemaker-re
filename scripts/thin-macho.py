"""Extract one architecture slice from a universal (fat) Mach-O. Usage: thin-macho.py <fat> <out> [cputype]"""
import struct, sys
CPU = {7: "i386", 18: "ppc", 0x01000007: "x86_64"}
d = open(sys.argv[1], "rb").read()
assert d[:4] == b"\xca\xfe\xba\xbe", "not a fat Mach-O"
n, = struct.unpack_from(">I", d, 4)
want = int(sys.argv[3], 0) if len(sys.argv) > 3 else 7
for i in range(n):
    cpu, sub, off, size, align = struct.unpack_from(">iiIII", d, 8 + 20 * i)
    print(f"slice {i}: {CPU.get(cpu, hex(cpu))} offset=0x{off:x} size={size}")
    if cpu == want:
        open(sys.argv[2], "wb").write(d[off:off + size])
        print(f"  -> wrote {sys.argv[2]}")
