#!/usr/bin/env python3
"""Parse and re-emit the Pacemaker device file .Pacemaker/settings.dat.

The file is a boost::serialization *text* archive (archive version 4, so a boost ~1.34-era
text_oarchive) of `SettingRepository`, written by `SettingRepository::priv_save` in pmplayer.
See notes/settings-dat.md for the grammar and the evidence.

    python -I scripts/settingsdat.py FILE              # flat decoded table
    python -I scripts/settingsdat.py FILE --check      # parse + re-emit, must be byte-identical
    python -I scripts/settingsdat.py FILE --json       # machine-readable
    python -I scripts/settingsdat.py FILE --names      # menu tree with titles and decoded values
    python -I scripts/settingsdat.py FILE --changed    # only settings that differ from the default

--names/--changed read the UI strings from the firmware's english.lang (default
work/fw-180096069/bin/resources/fonts/english.lang, or --lang PATH).  The id -> title/parent/default
tables below were read out of pmplayer 180096069 (the PMSettingsAlways/PlayMode/MixMode and
SettingMediator constructors); see notes/settings-dat.md.

As a module: parse(text) -> Archive, emit(archive) -> text.  Doubles are written the way libstdc++
writes a double with precision 17 and default float format, which is '%.17g'.
"""
import json
import os
import re
import sys

SIGNATURE = "22 serialization::archive"

# Pointer-class ids as boost assigns them: SettingRepository::serialize registers the five
# pointer serializers in this order right after the repository itself (class id 0).
BOOL, FLOAT, FLOATLIST, PARENT, STRING = 1, 2, 3, 4, 5
CLASS_NAMES = {BOOL: "BoolSetting", FLOAT: "FloatSetting", FLOATLIST: "FloatListSetting",
               PARENT: "ParentSetting", STRING: "StringSetting"}
# classes whose serialize() writes one value after the Setting base fields
VALUE_KIND = {BOOL: "bool", FLOAT: "double", FLOATLIST: "int", PARENT: None, STRING: None}


class FormatError(ValueError):
    pass


class Item:
    """One Setting*: base fields (id, parent id, MRU stamp) plus the per-class value."""
    __slots__ = ("cls", "id", "parent", "mru", "value")

    def __init__(self, cls, id, parent, mru, value=None):
        self.cls, self.id, self.parent, self.mru, self.value = cls, id, parent, mru, value

    def as_dict(self):
        return {"class": CLASS_NAMES[self.cls], "id": self.id, "parent": self.parent,
                "mru": self.mru, "value": self.value}


class Archive:
    def __init__(self):
        self.lib_version = 4            # boost archive library version token
        self.repo_tracking = 0          # class info of SettingRepository (class_info() == 1)
        self.repo_version = 16          # SettingRepository class version (13 in fw 14142)
        self.vec_tracking = 0           # class info of std::vector<Setting*>
        self.vec_version = 0
        self.item_version = 0           # collection item_version (written when library > 3)
        self.class_info = {}            # pointer class id -> (tracking, version), first use
        self.setting_tracking = 0       # class info of the Setting base, first use
        self.setting_version = 0
        self.items = []
        self.mru_counter = 0            # SettingRepository +0x4c, written after the vector


def _fmt_double(v):
    return "%.17g" % v


def _num(tok, kind):
    if kind == "bool":
        if tok not in ("0", "1"):
            raise FormatError("bool token %r" % tok)
        return tok == "1"
    if kind == "int":
        return int(tok)
    return float(tok)


def parse(text):
    toks = text.split()
    # 'serialization::archive' is one token; the signature is preceded by its length (22)
    if toks[:2] != ["22", "serialization::archive"]:
        raise FormatError("not a boost text archive")
    pos = [2]

    def nxt():
        if pos[0] >= len(toks):
            raise FormatError("truncated archive")
        t = toks[pos[0]]
        pos[0] += 1
        return t

    a = Archive()
    a.lib_version = int(nxt())
    a.repo_tracking = int(nxt())
    a.repo_version = int(nxt())
    a.vec_tracking = int(nxt())
    a.vec_version = int(nxt())
    count = int(nxt())
    a.item_version = int(nxt())
    first_setting = True
    for n in range(count):
        cid = int(nxt())
        if cid not in CLASS_NAMES:
            raise FormatError("unknown class id %d at item %d" % (cid, n))
        if cid not in a.class_info:
            a.class_info[cid] = (int(nxt()), int(nxt()))
        oid = int(nxt())
        if oid != n:
            raise FormatError("object id %d at item %d" % (oid, n))
        if first_setting:
            a.setting_tracking, a.setting_version = int(nxt()), int(nxt())
            first_setting = False
        sid, parent, mru = int(nxt()), int(nxt()), int(nxt())
        kind = VALUE_KIND[cid]
        value = _num(nxt(), kind) if kind else None
        a.items.append(Item(cid, sid, parent, mru, value))
    a.mru_counter = int(nxt())
    if pos[0] != len(toks):
        raise FormatError("%d trailing tokens" % (len(toks) - pos[0]))
    return a


def emit(a):
    cur = "%s %d %d %d %d %d %d %d" % (SIGNATURE, a.lib_version, a.repo_tracking,
                                       a.repo_version, a.vec_tracking, a.vec_version,
                                       len(a.items), a.item_version)
    lines = []
    seen = set()
    for n, it in enumerate(a.items):
        # the pointer's class id (plus class info on first use) ends the previous line ...
        if it.cls not in seen:
            t, v = a.class_info.get(it.cls, (1, 0))
            cur += " %d %d %d" % (it.cls, t, v)
            seen.add(it.cls)
        else:
            cur += " %d" % it.cls
        lines.append(cur)
        # ... and boost starts a new line before every object id
        cur = "%d" % n
        if n == 0:
            cur += " %d %d" % (a.setting_tracking, a.setting_version)
        cur += " %d %d %d" % (it.id, it.parent, it.mru)
        kind = VALUE_KIND[it.cls]
        if kind == "bool":
            cur += " %d" % (1 if it.value else 0)
        elif kind == "int":
            cur += " %d" % it.value
        elif kind == "double":
            cur += " " + _fmt_double(it.value)
    lines.append(cur + " %d" % a.mru_counter)
    return "\n".join(lines)


# ---------------------------------------------------------------------------------------------
# What the ids mean.  Source: constructors in pmplayer 180096069.  Tuple layouts:
#   FOLDERS[id]  = (title string id, parent id)                     -> ParentSetting (menu folder)
#   SETTINGS[id] = (kind, title string id, parent id, default, subtype, creator)
# kind: B bool, F double, L index into a list, S string (never persisted: value is not in the file)
# title string id 0 = no title (hidden setting).  creator: which class builds it.
FOLDERS = {
    1: (3, 0), 2: (39, 0), 3: (40, 2), 4: (46, 2), 5: (56, 0), 6: (217, 5), 7: (218, 5),
    8: (113, 7), 200: (220, 8), 201: (219, 7), 400: (245, 201), 401: (221, 5), 402: (222, 401),
    600: (10, 401), 601: (223, 401), 602: (224, 601), 603: (96, 401), 604: (32, 5),
    2400: (206, 0), 2402: (207, 2400), 2403: (208, 2400),
}
SETTINGS = {
    100: ('L', 22, 8, 0, 104, 'always'), 101: ('F', 23, 8, 6.2831854820251465, None, 'always'),
    300: ('F', 1, 201, 2.284794807434082, None, 'mix'), 301: ('B', 269, 201, False, 1, 'play'),
    500: ('B', 225, 401, True, 6, 'always'),
    700: ('S', 97, 603, '', None, 'always'), 701: ('S', 98, 603, '', None, 'always'),
    702: ('S', 101, 603, '', None, 'always'),
    800: ('L', 7, 1, 0, 100, 'play'), 801: ('B', 8, 1, False, 1, 'play'),
    900: ('S', 59, 0, '>', None, 'always'), 901: ('F', 1, 0, 2.284794807434082, None, 'mix'),
    1000: ('S', 41, 3, '33', None, 'always'), 1001: ('S', 42, 3, '', None, 'always'),
    1002: ('S', 43, 3, '', None, 'always'), 1003: ('S', 44, 3, '', None, 'mediator'),
    1004: ('S', 45, 3, '', None, 'mediator'),
    1100: ('S', 47, 4, '', None, 'always'), 1101: ('S', 48, 4, '', None, 'always'),
    1102: ('S', 50, 4, '', None, 'always'), 1103: ('S', 51, 4, '', None, 'always'),
    1104: ('S', 52, 4, '', None, 'mediator'), 1200: ('S', 53, 2, '', None, 'mediator'),
    1300: ('B', 5, 6, True, 1, 'mix'), 1301: ('L', 270, 6, 0, 113, 'mix'),
    1302: ('B', 271, 6, True, 1, 'mix'), 1303: ('L', 272, 6, 3, 114, 'mix'),
    1304: ('B', 6, 6, True, 1, 'mix'), 1305: ('B', 242, 6, True, 1, 'mix'),
    1306: ('B', 255, 6, False, 1, 'mix'), 1307: ('L', 238, 6, 0, 111, 'mix'),
    1308: ('B', 60, 6, True, 5, 'mix'), 1309: ('L', 243, 6, 1, 112, 'mix'),
    1310: ('B', 37, 6, True, 0, 'mix'),
    1400: ('F', 106, 200, 0.0, None, 'always'), 1401: ('F', 105, 200, 0.0, None, 'always'),
    1402: ('F', 104, 200, 0.0, None, 'always'),
    1500: ('F', 106, 400, 0.0, None, 'always'), 1501: ('F', 105, 400, 0.0, None, 'always'),
    1502: ('F', 104, 400, 0.0, None, 'always'),
    1600: ('B', 4, 7, True, 1, 'always'),
    1700: ('B', 31, 402, True, 0, 'always'), 1701: ('B', 26, 402, True, 0, 'always'),
    1702: ('B', 30, 402, True, 0, 'always'), 1703: ('B', 29, 402, True, 0, 'always'),
    1704: ('B', 28, 402, True, 0, 'always'), 1705: ('B', 27, 402, False, 0, 'always'),
    1706: ('B', 25, 402, True, 0, 'always'), 1707: ('B', 246, 402, False, 0, 'always'),
    1708: ('B', 250, 402, False, 0, 'always'), 1709: ('B', 251, 402, False, 0, 'always'),
    1710: ('B', 252, 402, False, 0, 'always'), 1711: ('B', 253, 402, False, 0, 'always'),
    1712: ('B', 249, 402, False, 0, 'always'), 1713: ('B', 254, 402, False, 0, 'always'),
    1800: ('B', 11, 600, True, 4, 'always'), 1801: ('B', 12, 600, True, 1, 'mix'),
    1900: ('L', 202, 602, 2, 110, 'mix'), 1901: ('L', 203, 602, 1, 110, 'mix'),
    1902: ('L', 204, 602, 3, 110, 'mix'), 1903: ('L', 205, 602, 0, 110, 'mix'),
    1904: ('L', 198, 602, 4, 110, 'mix'), 1905: ('L', 199, 602, 6, 110, 'mix'),
    1906: ('L', 200, 602, 5, 110, 'mix'), 1907: ('L', 201, 602, 7, 110, 'mix'),
    1908: ('S', 197, 601, '>', None, 'mix'),
    2000: ('L', 33, 604, 2, 105, 'always'), 2001: ('L', 14, 604, 1, 101, 'always'),
    2002: ('L', 15, 604, 2, 102, 'always'), 2003: ('B', 17, 604, False, 1, 'always'),
    2004: ('B', 18, 604, True, 103, 'always'), 2005: ('S', 41, 604, '33', None, 'always'),
    2100: ('S', 55, 5, '', None, 'mediator'),
    2200: ('F', 0, 5, 3.1415927410125732, None, 'play'),
    2201: ('F', 0, 5, 3.1415927410125732, None, 'play'),
    2202: ('F', 0, 5, 2.284794807434082, None, 'mix'),
    2203: ('S', 57, 5, '', None, 'mix'), 2204: ('S', 58, 5, '', None, 'play'),
    2205: ('B', 129, 5, False, 1, 'mix'), 2206: ('S', 130, 0, '>', None, 'mix'),
    2300: ('S', 64, 0, '>', None, 'play'), 2301: ('S', 65, 0, '>', None, 'play'),
    2404: ('S', 209, 2400, '', None, 'always'),
}
# FloatListSetting::setupValidValues(subtype): label string ids by index, and the number each
# index stands for (absent = the value is just the index).
LIST_LABELS = {
    100: [71, 76, 78], 101: [79, 80, 81], 102: [71, 87, 88], 104: [82, 83], 105: [71, 85, 86],
    110: [107, 105, 106, 104, 112, 109, 111, 108, 234, 235, 236, 237, 246],
    111: [239, 240, 241], 112: [71, 70],
    113: [71] + list(range(273, 282)), 114: list(range(275, 283)),
}
LIST_VALUES = {100: [0, 1, 2], 101: [60, 125, 200], 102: [0, 10, 30], 104: [1, 0],
               105: [0, 60, 300], 111: [0, 1, 2], 112: [0, 1]}
# BoolSetting::setupValidValues(subtype): (label string id for false, for true)
BOOL_LABELS = {0: (71, 70), 1: (71, 70), 2: (0, 0), 4: (75, 74), 5: (69, 68), 6: (226, 227),
               103: (73, 72)}


def load_lang(path):
    """english.lang: `N "text"` lines; `//` starts a comment."""
    names = {}
    with open(path, encoding="utf-8") as f:
        for line in f:
            m = re.match(r'\s*(\d+)\s+"(.*?)"', line)
            if m:
                names[int(m.group(1))] = m.group(2)
    return names


def _default_lang():
    here = os.path.dirname(os.path.abspath(__file__))
    return os.path.join(here, "..", "work", "fw-180096069", "bin", "resources", "fonts",
                        "english.lang")


def describe(it, lang):
    """(title, value text) for one parsed Item, using the tables above."""
    def s(i):
        return lang.get(i, "#%d" % i) if i else ""
    if it.id in FOLDERS:
        return s(FOLDERS[it.id][0]), ""
    info = SETTINGS.get(it.id)
    if not info:
        return "?", "" if it.value is None else str(it.value)
    kind, title, _par, _dflt, sub, _who = info
    if kind == "B":
        lab = BOOL_LABELS.get(sub, (0, 0))[1 if it.value else 0]
        return s(title), ("%s (%d)" % (s(lab), it.value) if lab else str(int(it.value)))
    if kind == "L":
        labs = LIST_LABELS.get(sub, [])
        vals = LIST_VALUES.get(sub)
        txt = s(labs[it.value]) if 0 <= it.value < len(labs) else "?"
        extra = " -> %s" % vals[it.value] if vals and 0 <= it.value < len(vals) else ""
        return s(title), "[%d] %s%s" % (it.value, txt, extra)
    if kind == "F":
        return s(title), _fmt_double(it.value)
    return s(title), ""


def names_table(a, lang, changed_only=False):
    children = {}
    for it in a.items:
        children.setdefault(it.parent, []).append(it)
    ids = {it.id for it in a.items}
    out = []

    def walk(pid, depth):
        for it in sorted(children.get(pid, []), key=lambda i: i.id):
            title, val = describe(it, lang)
            info = SETTINGS.get(it.id)
            if changed_only:
                if info and info[0] != "S" and it.value != info[3]:
                    dflt = ("[%d]" % info[3]) if info[0] == "L" else (
                        _fmt_double(info[3]) if info[0] == "F" else str(int(info[3])))
                    out.append("%-5d %-24s %s   (default %s)  mru=%d" % (
                        it.id, title, val, dflt, it.mru))
            else:
                out.append("%s%-5d %-*s %-7s mru=%-5d %s" % (
                    "  " * depth, it.id, max(24 - 2 * depth, 1), title or "(hidden)",
                    {BOOL: "bool", FLOAT: "double", FLOATLIST: "list", PARENT: "folder",
                     STRING: "string"}[it.cls], it.mru, val))
            walk(it.id, depth + 1)
    walk(0, 0)
    for it in a.items:
        if it.parent != 0 and it.parent not in ids:
            out.append("-- id %d has parent %d, which is not in the file" % (it.id, it.parent))
    return "\n".join(out)


def table(a):
    byid = {it.id: it for it in a.items}
    lines = []
    for it in a.items:
        depth, p = 0, it.parent
        while p in byid and depth < 20:
            depth += 1
            p = byid[p].parent
        val = "" if it.value is None else (_fmt_double(it.value) if isinstance(it.value, float)
                                           else str(int(it.value)))
        lines.append("%s%-5d %-16s parent=%-5d mru=%-5d %s" % (
            "  " * depth, it.id, CLASS_NAMES[it.cls], it.parent, it.mru, val))
    lines.append("MRU counter: %d" % a.mru_counter)
    return "\n".join(lines)


def main(argv):
    sys.stdout.reconfigure(encoding="utf-8", errors="replace")
    args = [x for x in argv[1:] if not x.startswith("--")]
    if "--lang" in argv:
        args.remove(argv[argv.index("--lang") + 1])
    if len(args) != 1:
        sys.exit(__doc__)
    with open(args[0], "rb") as f:
        raw = f.read()
    a = parse(raw.decode("ascii"))
    if "--check" in argv:
        again = emit(a).encode("ascii")
        if again != raw:
            n = next((i for i, (x, y) in enumerate(zip(again, raw)) if x != y),
                     min(len(again), len(raw)))
            sys.exit("MISMATCH at byte %d (lengths %d vs %d)" % (n, len(again), len(raw)))
        print("OK: %d settings, %d bytes, re-emitted byte-identical" % (len(a.items), len(raw)))
    elif "--names" in argv or "--changed" in argv:
        lp = argv[argv.index("--lang") + 1] if "--lang" in argv else _default_lang()
        lang = load_lang(lp) if os.path.exists(lp) else {}
        print(names_table(a, lang, changed_only="--changed" in argv))
        print("MRU counter: %d" % a.mru_counter)
    elif "--json" in argv:
        json.dump({"repo_version": a.repo_version, "mru_counter": a.mru_counter,
                   "settings": [it.as_dict() for it in a.items]}, sys.stdout, indent=1)
        print()
    else:
        print(table(a))


if __name__ == "__main__":
    main(sys.argv)
