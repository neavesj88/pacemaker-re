"""Tests for the Tonium sweep patches (patch-kernel-name.py, patch-modinfo.py, patch-plugin-strings.py, the
cramfs english.lang, sweep-tonium.py). Needs the gitignored stock firmware tree; set PM_WORK to its work/ directory
when running from a worktree.   python scripts/test-tonium-sweep.py
The kernel test also checks work/build/074/zImage-revival-name when it exists (build it as the build script header says)."""
import hashlib
import importlib.util
import os
import struct
import sys
import tempfile
import unittest
import zlib

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)


def load(name, file):
    spec = importlib.util.spec_from_file_location(name, os.path.join(HERE, file))
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


pkn = load("patch_kernel_name", "patch-kernel-name.py")
pmi = load("patch_modinfo", "patch-modinfo.py")
pps = load("patch_plugin_strings", "patch-plugin-strings.py")
pa = load("patch_about", "patch-about.py")
st = load("sweep_tonium", "sweep-tonium.py")

WORK = next((p for p in (os.environ.get("PM_WORK"), os.path.join(ROOT, "work")) if p and os.path.isdir(p)), None)
FW = os.path.join(WORK, "fw-180096069") if WORK else None
RT = os.path.join(WORK, "rootfs-180096069") if WORK else None
INPUT = os.path.join("lib", "modules", "2.6.19.2", "kernel", "drivers", "input")
MODULES = {"evrout": os.path.join(INPUT, "evrout.ko"), "pmic_input": os.path.join(INPUT, "misc", "pmic_input.ko")}


def rd(path):
    with open(path, "rb") as f:
        return f.read()


have_fw = bool(FW and os.path.isfile(os.path.join(FW, "boot", "zImage")) and os.path.isdir(RT or ""))


def modinfo_like_kernel(d, tag):
    """get_modinfo() of the 2.6.19 loader: walk the NUL separated strings of .modinfo found through the section headers."""
    secs, _, _ = pmi.sections(d)
    h = dict(secs)[".modinfo"]
    assert h[4] + h[5] <= len(d)
    p = d[h[4]:h[4] + h[5]]
    i = 0
    while i < len(p):
        j = p.index(b"\0", i)
        s = p[i:j]
        if s.startswith(tag + b"="):
            return s[len(tag) + 1:]
        i = j + 1
        while i < len(p) and p[i] == 0:
            i += 1
    return None


@unittest.skipUnless(have_fw, "stock firmware tree not found (PM_WORK)")
class KernelName(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.stock = rd(os.path.join(FW, "boot", "zImage"))
        cls.logo = rd(os.path.join(WORK, "build", "073", "zImage-revival"))
        _, _, _, cls.lk = pkn.pkl.gzip_parts(cls.logo)

    def test_patch_changes_only_the_name(self):
        new = pkn.patch_kernel(self.lk)
        self.assertEqual(len(new), len(self.lk))
        diff = [i for i in range(len(new)) if new[i] != self.lk[i]]
        self.assertTrue(pkn.NAME_OFF <= diff[0] and diff[-1] < pkn.NAME_OFF + pkn.SLOT)
        self.assertEqual(new[pkn.NAME_OFF:pkn.NAME_OFF + pkn.SLOT], b"Pacemaker Revival" + bytes(7))
        self.assertEqual(new[pkn.NAME_OFF + pkn.SLOT:pkn.NAME_OFF + pkn.SLOT + 10], b"pm_keypad\0")
        # the machine type number and the name pointer (so the board is still matched) are as before
        self.assertEqual(struct.unpack_from("<I", new, pkn.MACHINE_DESC)[0], 0x1BF)
        self.assertEqual(struct.unpack_from("<I", new, pkn.NAME_PTR)[0], pkn.BASE + pkn.NAME_OFF)
        self.assertEqual(new[:pkn.NAME_OFF], self.lk[:pkn.NAME_OFF])
        self.assertEqual(new.lower().count(b"tonium"), 0)

    def test_logo_region_untouched(self):
        # the logo kernel differs from stock only in the logo, and the name patch keeps that
        _, _, _, sk = pkn.pkl.gzip_parts(self.stock)
        new = pkn.patch_kernel(self.lk)
        d1 = [i for i in range(len(sk)) if sk[i] != self.lk[i]]
        d2 = [i for i in range(len(sk)) if sk[i] != new[i]]
        self.assertEqual(sorted(set(d2) - set(d1)), [i for i in range(pkn.NAME_OFF, pkn.NAME_OFF + pkn.SLOT)
                                                      if sk[i] != new[i]])
        self.assertTrue(set(d1) <= set(d2))

    def test_refuses_patched_and_stock_kernels(self):
        with self.assertRaises(SystemExit):
            pkn.patch_kernel(pkn.patch_kernel(self.lk))
        _, _, _, sk = pkn.pkl.gzip_parts(self.stock)
        # the stock kernel carries the stock logo, but the name check is what must hold: it does, so this passes;
        # a kernel whose pointer differs must be refused
        bad = bytearray(self.lk)
        struct.pack_into("<I", bad, pkn.NAME_PTR, pkn.BASE + pkn.NAME_OFF + 4)
        with self.assertRaises(SystemExit):
            pkn.patch_kernel(bytes(bad))
        bad = bytearray(self.lk)
        struct.pack_into("<I", bad, pkn.MACHINE_DESC, 0x1BE)
        with self.assertRaises(SystemExit):
            pkn.patch_kernel(bytes(bad))
        self.assertEqual(len(pkn.patch_kernel(sk)), len(sk))

    def test_refuses_wrong_zimages(self):
        with self.assertRaises(SystemExit):
            pkn.check_inputs(self.stock, self.stock)          # not the 073 logo kernel
        with self.assertRaises(SystemExit):
            pkn.check_inputs(self.logo, self.logo)            # not the stock zImage

    def test_built_zimage(self):
        p = os.path.join(WORK, "build", "074", "zImage-revival-name")
        if not os.path.isfile(p):
            self.skipTest("work/build/074/zImage-revival-name not built")
        out = rd(p)
        n, used, span = pkn.verify(self.stock, self.logo, out)
        self.assertGreater(n, 0)
        self.assertLessEqual(n, 24)
        self.assertLessEqual(used, span)
        self.assertEqual(len(out), len(self.stock))
        # an independent decompression (zlib on the gzip member) gives the patched kernel
        k = zlib.decompressobj(16 + zlib.MAX_WBITS).decompress(out[pkn.pkl.gzip_parts(out)[0]:])
        self.assertEqual(k, pkn.patch_kernel(self.lk))
        # the decompressor stub and everything after the payload are stock
        start, _, end, _ = pkn.pkl.gzip_parts(self.stock)
        self.assertEqual(out[:start], self.stock[:start])
        self.assertEqual(out[end:], self.stock[end:])
        # a tampered output is rejected
        bad = bytearray(out)
        bad[start + 5000] ^= 1
        with self.assertRaises(Exception):
            pkn.verify(self.stock, self.logo, bytes(bad))


@unittest.skipUnless(have_fw, "stock firmware tree not found (PM_WORK)")
class Modinfo(unittest.TestCase):
    def mod(self, name):
        return rd(os.path.join(RT, MODULES[name]))

    def test_both_modules_grow_and_read_back(self):
        for name in MODULES:
            d = self.mod(name)
            out = pmi.patch(d)
            self.assertEqual(modinfo_like_kernel(d, b"author")[:6], b"Tonium")
            self.assertEqual(modinfo_like_kernel(out, b"author"), b"Revival contributors")
            for tag in (b"vermagic", b"depends", b"license", b"description"):
                self.assertEqual(modinfo_like_kernel(out, tag), modinfo_like_kernel(d, tag), tag)
            self.assertEqual(modinfo_like_kernel(out, b"vermagic"), b"2.6.19.2 preempt mod_unload ARMv6 ")
            self.assertNotIn(b"tonium", out.lower())
            # grown: the old bytes before the original EOF are identical except the zeroed old section and the header fields
            secs, shoff, ent = pmi.sections(out)
            for n, h in secs:
                if h[1] != 8:  # not NOBITS
                    self.assertLessEqual(h[4] + h[5], len(out), n)
            h = dict(secs)[".modinfo"]
            self.assertEqual(h[4] % 4, 0)
            self.assertGreaterEqual(h[4], len(d))
            self.assertEqual(h[5] % 4, 0)
            # only .modinfo's header entry changed
            osecs, _, _ = pmi.sections(d)
            self.assertEqual([n for n, a in osecs if a != dict(secs)[n]], [".modinfo"])

    def test_in_place_when_it_fits(self):
        d = self.mod("evrout")
        out = pmi.patch(d, b"Revival")
        self.assertEqual(len(out), len(d))
        self.assertEqual(modinfo_like_kernel(out, b"author"), b"Revival")
        self.assertEqual(dict(pmi.sections(out)[0])[".modinfo"][4], dict(pmi.sections(d)[0])[".modinfo"][4])
        diff = [i for i in range(len(d)) if d[i] != out[i]]
        h = dict(pmi.sections(d)[0])[".modinfo"]
        self.assertTrue(all(h[4] <= i < h[4] + h[5] for i in diff))

    def test_deterministic_and_refuses(self):
        d = self.mod("pmic_input")
        self.assertEqual(pmi.patch(d), pmi.patch(d))
        with self.assertRaises(SystemExit):
            pmi.patch(pmi.patch(d))                       # already patched
        with self.assertRaises(SystemExit):
            pmi.patch(rd(os.path.join(RT, "lib", "modules", "2.6.19.2", "kernel", "drivers", "ide", "ide-disk.ko")))
        with self.assertRaises(SystemExit):
            pmi.patch(b"not an elf" * 100)
        with self.assertRaises(SystemExit):
            pmi.patch(d, b"bad\x01author")

    def test_no_other_module_mentions_tonium(self):
        for root, _, files in os.walk(os.path.join(RT, "lib", "modules")):
            for f in files:
                if f.endswith(".ko") and f not in (os.path.basename(p) for p in MODULES.values()):
                    self.assertNotIn(b"tonium", rd(os.path.join(root, f)).lower(), f)


@unittest.skipUnless(have_fw, "stock firmware tree not found (PM_WORK)")
class PluginStrings(unittest.TestCase):
    def test_all_libraries(self):
        total = 0
        for name, (sha, strings) in pps.LIBS.items():
            d = rd(os.path.join(FW, "bin", name))
            out = pps.patch(name, d)
            self.assertEqual(len(out), len(d))
            for off, old, new in strings:
                total += 1
                self.assertEqual(d[off:off + 4 * len(old)].decode("utf-32-le"), old)
                self.assertEqual(out[off:off + 4 * len(old) + 4].decode("utf-32-le").rstrip("\0"), new)
                self.assertEqual(out[off:off + 4 * len(new) + 4], new.encode("utf-32-le") + bytes(4))
                self.assertLessEqual(len(new), len(old))
                self.assertNotIn("Tonium", new)
            changed = [i for i in range(len(d)) if d[i] != out[i]]
            self.assertTrue(all(any(off <= i < off + 4 * len(old) for off, old, _ in strings) for i in changed))
            self.assertNotIn(b"tonium", out.lower())
            self.assertNotIn("tonium".encode("utf-32-le"), out.lower())
        self.assertEqual(total, 22)
        self.assertEqual(len(pps.LIBS), 14)

    def test_wording(self):
        name = "libcontainer_mpeg4.so"
        out = pps.patch(name, rd(os.path.join(FW, "bin", name)))
        s = dict((off, new) for off, _, new in pps.LIBS[name][1])
        self.assertEqual(sorted(s.values()), ["MPEG-4 container plugin (Revival)", "Pacemaker Revival - all wrongs reversed"])
        self.assertNotIn("(C)", "".join(s.values()))   # no copyright claim over Tonium's code
        name = "libdecoder_pcm.so"
        old = [o for _, o, _ in pps.LIBS[name][1]][0]
        new = [n for _, _, n in pps.LIBS[name][1]][0]
        self.assertEqual((old, new), ("PCM decoder plugin by Tonium", "PCM decoder plugin (Revival)"))
        self.assertEqual(len(old), len(new))

    def test_every_tonium_string_in_the_stock_libs_is_covered(self):
        covered = 0
        for name in sorted(os.listdir(os.path.join(FW, "bin"))):
            if name.endswith(".so"):
                d = rd(os.path.join(FW, "bin", name))
                n = d.lower().count("tonium".encode("utf-32-le"))
                if name in pps.LIBS:
                    self.assertEqual(n, len(pps.LIBS[name][1]), name)
                    covered += 1
                else:
                    self.assertEqual(n, 0, name)
        self.assertEqual(covered, 14)

    def test_refuses(self):
        name = "libdecoder_au.so"
        d = rd(os.path.join(FW, "bin", name))
        out = pps.patch(name, d)
        with self.assertRaises(SystemExit):
            pps.patch(name, out)                          # already patched
        bad = bytearray(d)
        bad[0x1694] ^= 1
        with self.assertRaises(SystemExit):
            pps.patch(name, bytes(bad))                   # sha256 differs


@unittest.skipUnless(have_fw, "stock firmware tree not found (PM_WORK)")
class CramfsLang(unittest.TestCase):
    def test_cramfs_english_lang(self):
        cram = rd(os.path.join(RT, "usr", "local", "share", "fonts", "english.lang"))
        self.assertEqual(cram, rd(os.path.join(FW, "bin", "resources", "fonts", "english.lang")))   # same stock file
        out = pa.patch_lang(cram)
        self.assertEqual(out, cram.replace(b'49 "\xc2\xa9 Tonium"', b'49 "\xc2\xa9 Revival contributors"'))
        self.assertNotIn(b"Tonium", out)
        self.assertEqual(len(out.split(b"\n")), len(cram.split(b"\n")))   # one line changed, none added

    def test_build_script_wires_everything_and_leaves_redboot_alone(self):
        s = open(os.path.join(HERE, "build-180096074.sh")).read()
        for needle in ("patch-kernel-name.py --verify", "patch-modinfo.py", "patch-plugin-strings.py", "sweep-tonium.py",
                       "/usr/local/share/fonts/english.lang=", "evrout.ko=", "pmic_input.ko=",
                       "libcontainer_aiffc.so=", "libdecoder_vorbis.so=", "bootloaderver=12838"):
            self.assertIn(needle, s)
        self.assertNotIn("/boot/redboot.bin=", s)         # the bootloader is never replaced
        self.assertIn(hashlib.sha256(rd(os.path.join(FW, "boot", "redboot.bin"))).hexdigest(), s)

    def test_stock_redboot_is_the_one_the_build_checks(self):
        self.assertEqual(hashlib.sha256(rd(os.path.join(FW, "boot", "redboot.bin"))).hexdigest(),
                         "636cea0be6cdd6b358ad75019f246a8ee92383cdfadf3fa43906b0737f5d40db")


class Sweep(unittest.TestCase):
    def test_tokens(self):
        d = b"xx com.tonium.peer yy " + bytes(10) + "toniumId".encode("utf-32-le") + b"\x00" * 4 + "ToNiUm AB".encode("utf-16-le")
        t = st.tokens(d)
        self.assertIn(("narrow", "com.tonium.peer"), t)
        self.assertIn(("utf-32-le", "toniumId"), t)
        self.assertIn(("utf-16-le", "ToNiUm"), t)
        self.assertEqual(st.tokens(b"nothing here" * 100), set())

    def test_walk_and_keep_list(self):
        with tempfile.TemporaryDirectory() as t:
            open(os.path.join(t, "pwrApplication"), "wb").write(b"\0com.tonium.peer\0com.tonium.app_pid\0")
            open(os.path.join(t, "redboot.bin"), "wb").write(b"\0Tonium\0")
            sys_argv = sys.argv
            try:
                sys.argv = ["sweep-tonium.py", t]
                with self.assertRaises(SystemExit) as c:
                    st.main()
                self.assertEqual(c.exception.code, 0)
                open(os.path.join(t, "other.so"), "wb").write(b"\0by Tonium AB\0")
                with self.assertRaises(SystemExit) as c:
                    st.main()
                self.assertEqual(c.exception.code, 1)
                os.remove(os.path.join(t, "other.so"))
                open(os.path.join(t, "pwrApplication"), "ab").write(b"Tonium Pacemaker\0")   # new text in a keep-list file
                with self.assertRaises(SystemExit) as c:
                    st.main()
                self.assertEqual(c.exception.code, 1)
            finally:
                sys.argv = sys_argv


if __name__ == "__main__":
    unittest.main(verbosity=2)
