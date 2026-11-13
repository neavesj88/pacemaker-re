"""Tests for scripts/patch-about.py. Needs the gitignored stock firmware tree (work/fw-180096069); set
PM_WORK to its work/ directory when running from a worktree.  python3 scripts/test-patch-about.py"""
import hashlib
import importlib.util
import os
import struct
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
spec = importlib.util.spec_from_file_location("patch_about", os.path.join(HERE, "patch-about.py"))
pa = importlib.util.module_from_spec(spec)
spec.loader.exec_module(pa)

WORK = next((p for p in (os.environ.get("PM_WORK"), os.path.join(ROOT, "work")) if p and os.path.isdir(p)), None)
FW = os.path.join(WORK, "fw-180096069") if WORK else None


def rd(path):
    with open(path, "rb") as f:
        return f.read()


def cstr(d, off):
    return d[off:d.index(b"\0", off)]


def deref(d, pool):
    """Follow the pointer word stored at file offset `pool` to the C string it points at."""
    return cstr(d, struct.unpack_from("<I", d, pool)[0] - pa.VA)


@unittest.skipUnless(FW and os.path.isfile(os.path.join(FW, "bin", "pmplayer")), "stock firmware tree not found (PM_WORK)")
class PmplayerPatch(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.stock = rd(os.path.join(FW, "bin", "pmplayer"))
        cls.legal = rd(pa.LEGAL)
        cls.out, cls.stock_legal = pa.apply(cls.stock, cls.legal)

    def test_size_and_determinism(self):
        self.assertEqual(len(self.out), pa.STOCK_SIZE)
        again, _ = pa.apply(self.stock, self.legal)
        self.assertEqual(again, self.out)

    def test_legal_reads_back_verbatim(self):
        self.assertEqual(deref(self.out, pa.LEGAL_POOL), self.legal.rstrip(b"\0"))
        self.assertEqual(deref(self.out, pa.LEGAL_POOL), self.legal)
        self.assertEqual(struct.unpack_from("<I", self.out, pa.LEGAL_POOL)[0],
                         struct.unpack_from("<I", self.stock, pa.LEGAL_POOL)[0], "Legal pool word must not move")

    def test_support_url(self):
        self.assertEqual(deref(self.out, pa.URL_POOL), b"github.com/neavesj88/pacemaker-re")

    def test_all_six_model_words(self):
        for off in pa.MODEL_DEFAULT + pa.MODEL_00 + pa.MODEL_01:
            self.assertEqual(deref(self.out, off), b"Pacemaker Revival", hex(off))

    def test_nothing_else_changed(self):
        allowed = [(pa.LEGAL_OFF, pa.OLD_URL_OFF + len(pa.OLD_URL))]
        allowed += [(o, o + 4) for o in (pa.URL_POOL,) + pa.MODEL_DEFAULT + pa.MODEL_00 + pa.MODEL_01]
        for i, (a, b) in enumerate(zip(self.stock, self.out)):
            if a != b:
                self.assertTrue(any(lo <= i < hi for lo, hi in allowed), f"byte {i:#x} changed outside the plan")
        # model literals and getModelName's pool words are untouched
        self.assertEqual(cstr(self.out, pa.P211_OFF), b"P211")
        self.assertEqual(cstr(self.out, pa.SIX66_OFF), b"666")
        for off in (0x5cd6e0 - pa.VA, 0x5cd6e8 - pa.VA):   # getModelName pool words, file offsets
            self.assertEqual(self.out[off:off + 4], self.stock[off:off + 4])

    def test_freed_block_layout(self):
        o = self.out
        end = pa.LEGAL_OFF + len(self.legal) + 1
        self.assertEqual(o[pa.LEGAL_OFF + len(self.legal)], 0)
        self.assertEqual(o[end:pa.MODEL_OFF], bytes(pa.MODEL_OFF - end))
        tail = pa.URL_OFF + len(pa.URL)
        self.assertEqual(o[tail:pa.OLD_URL_OFF + len(pa.OLD_URL)], bytes(pa.OLD_URL_OFF + len(pa.OLD_URL) - tail))
        self.assertNotIn(b"www.pacemaker.net", o)
        self.assertNotIn(b"Pacemaker Legal Notices", o)

    def test_strings_that_must_survive(self):
        for s in (b"/mnt/public/.Pacemaker/system.xml", b"com.tonium.peer", b"com.tonium.app_pid", b"toniumId"):
            self.assertEqual(self.out.count(s), self.stock.count(s), s)
        w = "Tonium;Device;".encode("utf-32-le")
        self.assertEqual(self.out.count(w), self.stock.count(w))
        self.assertGreater(self.out.count(w), 0)

    def test_stock_legal_extraction(self):
        self.assertEqual(len(self.stock_legal), pa.LEGAL_LEN)
        self.assertEqual(hashlib.sha256(self.stock_legal).hexdigest(), pa.LEGAL_SHA256)
        self.assertTrue(self.stock_legal.startswith(b"Pacemaker Legal Notices:"))

    def test_refuses_patched_input(self):
        with self.assertRaises(SystemExit):
            pa.apply(self.out, self.legal)

    def test_refuses_non_ascii_and_nul(self):
        for bad in (self.legal + "©".encode(), self.legal + b"\0"):
            with self.assertRaises(SystemExit):
                pa.apply(self.stock, bad)

    def test_refuses_each_pool_word_mismatch(self):
        for off in (pa.LEGAL_POOL, pa.URL_POOL) + pa.MODEL_DEFAULT + pa.MODEL_00 + pa.MODEL_01:
            mod = bytearray(self.stock)
            mod[off] ^= 0xFF
            with self.assertRaises(SystemExit):   # also trips the whole-file sha256 check
                pa.apply(bytes(mod), self.legal)


class LegalText(unittest.TestCase):
    def test_legal_txt(self):
        t = rd(pa.LEGAL)
        self.assertTrue(all(b < 0x7F and (b >= 0x20 or b == 10) for b in t), "ASCII + LF only")
        self.assertLessEqual(len(t) + 1, pa.MODEL_OFF - pa.LEGAL_OFF)
        self.assertNotIn(b"Tonium", t)
        self.assertIn(b"/bin/resources/licenses/", t)


@unittest.skipUnless(FW and os.path.isdir(os.path.join(FW, "bin", "resources", "fonts")), "stock firmware tree not found (PM_WORK)")
class LangFiles(unittest.TestCase):
    def test_lang_files(self):
        for name, vocal in (("english", True), ("japanese", True), ("korean", False)):
            stock = rd(os.path.join(FW, "bin", "resources", "fonts", name + ".lang"))
            want = stock.replace(b'49 "\xc2\xa9 Tonium"', b'49 "\xc2\xa9 Revival contributors"')
            if vocal:
                want = want.replace(b'236 "Wah"', b'236 "Vocal"')
            got = rd(os.path.join(ROOT, "device", "about", "lang", name + ".lang"))
            self.assertEqual(got, want, name)
            self.assertEqual(stock.count(b'49 "\xc2\xa9 Tonium"'), 1)
            self.assertNotIn(b"Tonium", got)
            self.assertEqual(pa.patch_lang(stock.replace(b'236 "Wah"', b'236 "Vocal"') if vocal else stock), got)

    def test_patch_lang_refuses(self):
        for bad in (b"", b'49 "\xc2\xa9 Tonium"\n49 "\xc2\xa9 Tonium"\n'):
            with self.assertRaises(SystemExit):
                pa.patch_lang(bad)


if __name__ == "__main__":
    unittest.main(verbosity=2)
