/* Stand-in for pmplayer.real, used under qemu-user with the device's ld.so and glibc 2.4.
 *
 * It reproduces exactly the things libvocalfx checks and patches: a non-PIE executable that exports
 * PeqAudioProcess::process (at 0x4f524c), the two destructors (0x4f5730, 0x4f58d8), the vtable (80 bytes at 0x877e48
 * with process in slot +0x1c) and AudioProcess::m_sampleRate, all at the firmware's real addresses. The stub "stock
 * process" mimics what the decompile of 0x4f524c shows: copyFrom(out,in), consume events, skip the filter when the
 * input is flagged silent, otherwise apply a stand-in "stock Wah" (gain 0.5 on both channels), then store the value
 * at this+0x80. Calls go through the vtable like the engine does (vptr[0x14]).
 *
 *   pmplayer.real MODE VALUE [BLOCKS [SLEEP_S]]    MODE: tone | side | bass | nonalias | silent
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <math.h>
#include <unistd.h>
#include "armstart.h"

#define VIS __attribute__((visibility("default"), used, noinline))
#define BLK 512

static volatile int g_orig_calls, g_stock_hits;

/* the vtable, same 80 bytes as the firmware's (read from work/fw-180096069/bin/pmplayer) */
__asm__(".section .peq_vt,\"ax\",%progbits\n.align 2\n"
        ".global _ZTVN7xponaut11audioengine15PeqAudioProcessE\n"
        ".type _ZTVN7xponaut11audioengine15PeqAudioProcessE,%object\n"
        "_ZTVN7xponaut11audioengine15PeqAudioProcessE:\n"
        ".word 0x0,0x877ec4,0x4f5730,0x4f58d8,0x495c44,0x4f4b60,0x495c4c,0x4f524c,0x4f4f24,0x4f60a0\n"
        ".word 0x495c98,0x495dac,0x4f4658,0x4f4660,0xffffffe8,0x877ec4,0x4f84ac,0x4f84b4,0x4f60ac,0x4f60b4\n"
        ".size _ZTVN7xponaut11audioengine15PeqAudioProcessE, 80\n.text\n");
extern uint32_t vtable[] __asm__("_ZTVN7xponaut11audioengine15PeqAudioProcessE");

VIS __attribute__((section(".peq_d1"))) void stub_d1(void *self) __asm__("_ZN7xponaut11audioengine15PeqAudioProcessD1Ev");
void stub_d1(void *self) { (void)self; }
VIS __attribute__((section(".peq_d0"))) void stub_d0(void *self) __asm__("_ZN7xponaut11audioengine15PeqAudioProcessD0Ev");
void stub_d0(void *self) { (void)self; }

__attribute__((visibility("default"), used)) double g_rate __asm__("_ZN7xponaut11audioengine12AudioProcess12m_sampleRateE") = 44100.0;

VIS __attribute__((section(".peq_proc")))
void stub_process(void *self, void *in, void *out, void *midi, void *ev) __asm__("_ZN7xponaut11audioengine15PeqAudioProcess7processERKNS0_18AudioProcessBufferERS2_RKN4juce10MidiBufferERNS0_11EventBufferE");
void stub_process(void *self, void *in, void *out, void *midi, void *ev)
{
    uint8_t *ib = in, *ob = out, *sp = self;
    float pending;
    (void)midi; (void)ev;
    g_orig_calls++;
    if (ib != ob) {                                       /* AudioProcessBuffer::copyFrom(out, in) */
        if (!ib[4]) {
            uint32_t n = *(uint32_t *)(ib + 0x38), c;
            *(uint32_t *)(ob + 0x28) = *(uint32_t *)(ib + 0x28);
            ob[4] = 0; *(uint32_t *)(ob + 0x38) = n;
            for (c = 0; c < *(uint32_t *)(ob + 0x34); c++) memcpy(*(void **)(ob + 8 + 4 * c), *(void **)(ib + 8 + 4 * c), n * 4);
        } else ob[4] = 1;
    }
    memcpy(&pending, sp + 0x7c, 4);                       /* stands in for the event-driven parameter */
    if (!ib[4] && !sp[4] && pending != 0.0f) {            /* the stock filter path */
        uint32_t n = *(uint32_t *)(ob + 0x38), i;
        float *l = *(float **)(ob + 8), *r = *(float **)(ob + 12);
        for (i = 0; i < n; i++) { l[i] *= 0.5f; r[i] *= 0.5f; }
        g_stock_hits++;
    }
    memcpy(sp + 0x80, &pending, 4);
}

static void call_slot(void *obj, void *in, void *out)
{
    uint32_t *vptr = *(uint32_t **)obj;                   /* vptr = vtable + 8 */
    ((void (*)(void *, void *, void *, void *, void *))(uintptr_t)vptr[0x14 / 4])(obj, in, out, 0, 0);
}

static void put(const char *s) { ssize_t w = write(1, s, strlen(s)); (void)w; }

int test_main(int argc, char **argv)
{
    static uint8_t obj[0x90], ib[0x70], ob[0x70];
    static float inL[BLK], inR[BLK], oL[BLK], oR[BLK], refL[BLK], refR[BLK];
    const char *mode = argc > 1 ? argv[1] : "tone";
    float val = argc > 2 ? (float)atof(argv[2]) : 0.0f;
    int blocks = argc > 3 ? atoi(argv[3]) : 200, b, i, aliased = strcmp(mode, "nonalias") != 0;
    double sin_in = 0, sin_out = 0;
    uint32_t slot_now = vtable[7];
    char line[400];

    *(uint32_t *)obj = (uint32_t)(uintptr_t)&vtable[2];   /* vptr */
    memcpy(obj + 0x7c, &val, 4);
    *(uint32_t *)(ib + 0x34) = 2; *(uint32_t *)(ib + 0x38) = BLK;
    *(uint32_t *)(ob + 0x34) = 2; *(uint32_t *)(ob + 0x38) = BLK;
    *(float **)(ib + 8) = inL; *(float **)(ib + 12) = inR;
    *(float **)(ob + 8) = oL; *(float **)(ob + 12) = oR;

    for (b = 0; b < blocks; b++) {
        for (i = 0; i < BLK; i++) {
            double t = (double)(b * BLK + i) / 44100.0;
            float v = (float)(0.5 * sin(2 * M_PI * (strcmp(mode, "bass") == 0 ? 60.0 : 1000.0) * t));
            inL[i] = v; inR[i] = strcmp(mode, "side") == 0 ? -v : v;
            refL[i] = inL[i]; refR[i] = inR[i];
        }
        if (aliased) {                                    /* the live chain: in == out, one buffer */
            memcpy(oL, inL, sizeof inL); memcpy(oR, inR, sizeof inR);
            *(float **)(ib + 8) = oL; *(float **)(ib + 12) = oR;
            ib[4] = strcmp(mode, "silent") == 0 ? 1 : 0;
            call_slot(obj, ib, ib);
            if (ib[4] != (strcmp(mode, "silent") == 0 ? 1 : 0)) { put("FAIL silent flag not restored\n"); return 2; }
        } else {
            ib[4] = 0; ob[4] = 1;
            call_slot(obj, ib, ob);
        }
        if (b >= blocks - 40)
            for (i = 0; i < BLK; i++) { sin_in += (double)refL[i] * refL[i] + (double)refR[i] * refR[i]; sin_out += (double)oL[i] * oL[i] + (double)oR[i] * oR[i]; }
    }
    snprintf(line, sizeof line, "RESULT mode=%s val=%.0f ratio=%.5f db=%.2f stock_hits=%d orig_calls=%d blocks=%d slot=%08x\n",
             mode, (double)val, sqrt(sin_out / (sin_in + 1e-30)), 20 * log10(sqrt(sin_out / (sin_in + 1e-30)) + 1e-12),
             g_stock_hits, g_orig_calls, blocks, slot_now);
    if (argc > 4) sleep((unsigned)atoi(argv[4]));         /* lets the library's log thread write its periodic line */
    put(line);
    return 0;
}
