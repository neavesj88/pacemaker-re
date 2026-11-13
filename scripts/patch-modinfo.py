"""Change the `author=` tag in the .modinfo section of a 2.6.19.2 kernel module (notes/about-screen.md, "Tonium sweep").

evrout.ko and pmic_input.ko carry `author=Tonium AB`. The tag is only shown by `modinfo`; the 2.6.19 loader
(load_module -> get_modinfo/next_string) walks the NUL-separated strings looking for vermagic, so only that tag,
`depends=` and `alias=` have any effect and they are left byte for byte as they are.

The new author is longer than the old one, so the strings are re-packed (one NUL between entries, the entry order
kept, NUL padding to a multiple of 4 as the section alignment says). If that fits in the old section it is written
in place and nothing else changes. If not (the case for "Revival contributors" in both modules) the section is
moved to the end of the file, 4-byte aligned, and only its sh_offset and sh_size in the section header are
updated; the old bytes are zeroed so no "Tonium" is left in the file. A relocatable .ko has no program headers and
no relocations refer to .modinfo. Its .symtab does hold local OBJECT symbols in that section (__mod_author45,
__mod_description46, __mod_license47, __mod_vermagic5, __module_depends, __mod_alias51: st_value = offset of the entry
in the section, st_size = length + 1). The loader never uses them (.modinfo has SHF_ALLOC cleared and 2.6.19
simplify_symbols only adds sh_addr), but they would be stale debug symbols, so their st_value and st_size are
rewritten to the new entry positions as well; those are the only other bytes that change.

  patch-modinfo.py IN.ko OUT.ko [AUTHOR]        default AUTHOR: Revival project (in place in both modules)
"""
import os
import struct
import sys

AUTHOR = b"Revival project"  # fits in place in both modules (no section move), see the docstring
OLD_AUTHORS = (b"Tonium AB.", b"Tonium AB")


def sections(d):
    """(list of (name, header fields as a list), e_shoff, e_shentsize) of an ELF32 little-endian ET_REL."""
    if d[:4] != b"\x7fELF" or d[4] != 1 or d[5] != 1:
        sys.exit("refusing: not an ELF32 little-endian file")
    if struct.unpack_from("<H", d, 0x10)[0] != 1 or struct.unpack_from("<H", d, 0x12)[0] != 40:
        sys.exit("refusing: not a relocatable ARM object (a kernel module)")
    shoff, = struct.unpack_from("<I", d, 0x20)
    shentsize, shnum, shstrndx = struct.unpack_from("<HHH", d, 0x2E)
    hdrs = [list(struct.unpack_from("<10I", d, shoff + i * shentsize)) for i in range(shnum)]
    names = []
    for h in hdrs:
        s = hdrs[shstrndx][4] + h[0]
        names.append(d[s:d.index(b"\0", s)].decode())
    return list(zip(names, hdrs)), shoff, shentsize


def entries(blob):
    """The NUL-separated strings of a .modinfo blob, as the loader's next_string() sees them."""
    return [e for e in blob.split(b"\0") if e]


def pack(es):
    blob = b"".join(e + b"\0" for e in es)
    return blob + bytes(-len(blob) % 4)


def modinfo_symbols(d, secs, i):
    """[(file offset of the symbol entry, st_value, st_size, name)] of the local OBJECT symbols defined in section i."""
    out = []
    for n, h in secs:
        if h[1] != 2:       # SHT_SYMTAB
            continue
        strtab = secs[h[6]][1][4]
        for k in range(h[5] // 16):
            at = h[4] + k * 16
            name, value, size, info, _, shndx = struct.unpack_from("<IIIBBH", d, at)
            if shndx == i and info & 0xF == 1:
                s = strtab + name
                out.append((at, value, size, d[s:d.index(b"\0", s)]))
    return out


def entry_layout(es):
    """{start offset: entry length + 1} of the NUL-terminated entries of pack(es)."""
    out, p = {}, 0
    for e in es:
        out[p] = len(e) + 1
        p += len(e) + 1
    return out


def patch(d, author=AUTHOR):
    """Return the patched module. Exits on any mismatch."""
    if any(c < 32 or c > 126 for c in author) or not author:
        sys.exit("refusing: author must be printable ASCII")
    secs, shoff, shentsize = sections(d)
    idx = [i for i, (n, _) in enumerate(secs) if n == ".modinfo"]
    if len(idx) != 1:
        sys.exit("refusing: expected exactly one .modinfo section")
    i = idx[0]
    h = secs[i][1]
    off, size = h[4], h[5]
    blob = d[off:off + size]
    es = entries(blob)
    authors = [e for e in es if e.startswith(b"author=")]
    if len(authors) != 1 or authors[0][len(b"author="):] not in OLD_AUTHORS:
        sys.exit("refusing: .modinfo does not hold exactly one stock `author=Tonium AB` tag (already patched?)")
    if d.lower().count(b"tonium") != 1:
        sys.exit("refusing: expected exactly one 'tonium' in the module")
    if not blob.endswith(b"\0"):
        sys.exit("refusing: .modinfo is not NUL terminated")
    new_es = [b"author=" + author if e == authors[0] else e for e in es]
    new = pack(new_es)
    out = bytearray(d)
    # the OBJECT symbols follow their entries: each must sit exactly on an old entry (st_size = len + 1)
    old_start, p = [], 0
    for e in es:
        p = blob.index(e + b"\0", p)
        old_start.append(p)
        p += len(e) + 1
    new_layout = entry_layout(new_es)
    new_start = sorted(new_layout)
    syms = modinfo_symbols(d, secs, i)
    for at, value, sym_size, name in syms:
        if value not in old_start or sym_size != len(es[old_start.index(value)]) + 1:
            sys.exit(f"refusing: symbol {name!r} does not describe one .modinfo entry")
        n = old_start.index(value)
        struct.pack_into("<II", out, at + 4, new_start[n], new_layout[new_start[n]])
    hdr_at = shoff + i * shentsize
    if len(new) <= size:
        out[off:off + size] = new + bytes(size - len(new))
        new_size = size
    else:
        new_off = len(d) + (-len(d) % 4)
        out[off:off + size] = bytes(size)                 # no "Tonium" left behind
        out += bytes(new_off - len(d)) + new
        new_size = len(new)
        struct.pack_into("<I", out, hdr_at + 16, new_off)   # sh_offset
    struct.pack_into("<I", out, hdr_at + 20, new_size)      # sh_size
    out = bytes(out)
    check(d, out, author)
    return out


def check(old, new, author):
    """Independent read-back with the loader's view: same tags except author, the rest of the file untouched."""
    osecs, oshoff, ent = sections(old)
    nsecs, nshoff, _ = sections(new)
    assert oshoff == nshoff and [n for n, _ in osecs] == [n for n, _ in nsecs]
    changed = [n for (n, a), (_, b) in zip(osecs, nsecs) if a != b]
    assert changed in ([], [".modinfo"]), changed   # in place: the header entry is unchanged too
    oh = dict(osecs)[".modinfo"]
    nh = dict(nsecs)[".modinfo"]
    oes = entries(old[oh[4]:oh[4] + oh[5]])
    nes = entries(new[nh[4]:nh[4] + nh[5]])
    assert nh[4] % nh[8] == 0 and nh[4] + nh[5] <= len(new)
    assert [e for e in oes if not e.startswith(b"author=")] == [e for e in nes if not e.startswith(b"author=")]
    assert [e for e in nes if e.startswith(b"author=")] == [b"author=" + author]
    assert [e.split(b"=")[0] for e in oes] == [e.split(b"=")[0] for e in nes], "tag order changed"
    assert new[nh[4] + nh[5] - 1] == 0, "section must end with a NUL"
    # every byte outside the old and new .modinfo ranges and the one header field pair is unchanged
    keep = bytearray(new[:len(old)])
    keep[oh[4]:oh[4] + oh[5]] = old[oh[4]:oh[4] + oh[5]]
    hdr = nshoff + [n for n, _ in nsecs].index(".modinfo") * ent
    keep[hdr + 16:hdr + 24] = old[hdr + 16:hdr + 24]
    if nh[4] < len(old):
        keep[nh[4]:nh[4] + nh[5]] = old[nh[4]:nh[4] + nh[5]]
    # the OBJECT symbols of .modinfo: same set, same names, in the same order; only st_value and st_size are new, and
    # each now describes exactly one entry of the new blob (st_size = length + 1, the entry NUL included)
    oidx = [n for n, _ in osecs].index(".modinfo")
    osyms, nsyms = modinfo_symbols(old, osecs, oidx), modinfo_symbols(new, nsecs, oidx)
    assert [(a, n) for a, _, _, n in osyms] == [(a, n) for a, _, _, n in nsyms], "symbol set changed"
    nblob = new[nh[4]:nh[4] + nh[5]]
    for (at, ov, osz, name), (_, nv, nsz, _) in zip(osyms, nsyms):
        keep[at + 4:at + 12] = old[at + 4:at + 12]
        assert nv + nsz <= nh[5] and nblob[nv + nsz - 1] == 0 and 0 not in nblob[nv:nv + nsz - 1], name
        assert nblob[nv:nv + nsz - 1] in nes, name
        o_entry = old[oh[4] + ov:oh[4] + ov + osz - 1]
        n_entry = nblob[nv:nv + nsz - 1]
        assert (o_entry == n_entry) == (not name.startswith(b"__mod_author")), name   # only the author entry changed
    assert bytes(keep) == old, "bytes changed outside .modinfo, its header entry and the .modinfo symbols"
    assert b"tonium" not in new.lower()


def main():
    if len(sys.argv) not in (3, 4):
        sys.exit(__doc__)
    d = open(sys.argv[1], "rb").read()
    author = sys.argv[3].encode("ascii") if len(sys.argv) == 4 else AUTHOR
    out = patch(d, author)
    open(sys.argv[2], "wb").write(out)
    print(f"{os.path.basename(sys.argv[1])}: author -> {author.decode()}; {len(d)} -> {len(out)} bytes")


if __name__ == "__main__":
    main()
