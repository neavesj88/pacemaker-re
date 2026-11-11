/* Reference run of the device's Vocal filter for the equivalence test of crates/pm-mixer/src/vocal.rs.
 *
 *   vocal_ref <scenario.txt> <out.f32>
 *
 * Runs the device's float DSP (device/vocalfx/vocal_dsp.c, unmodified, linked in) through the scenario
 * and writes every output frame as two little-endian float32 (L, R). The Rust test
 * (tests/vocal_equiv.rs) generates the same input, runs its port and compares against the file this
 * writes. Build and run with gen.sh in this folder; it records the checksums of the C sources used.
 *
 * Why the input is built the way it is: the generator must produce bit-identical input in C and in
 * Rust, so it uses only IEEE +, - and * (no libm): the sines are digital resonators
 * y[n+1] = c*y[n] - y[n-1] whose coefficient c = 2cos(w) is a decimal literal, and the noise is the
 * 32-bit LCG of the device's own dsp_test.c. Build with -ffp-contract=off so no multiply-add is fused.
 *
 * The Wah value -> (target, engaged) mapping below is the code of vocal_process() in vocalfx.c
 * (that file cannot be built natively: it patches a vtable of the ARM firmware), transcribed line
 * for line, with vfx_engage() taken from the DSP's own header.
 */
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "vocal_dsp.h"

#define MAXBLK 4096

typedef struct { double c, y1, y2; } osc_t;
static void osc_init(osc_t *o, double c, double sinw) { o->c = c; o->y1 = 0.0; o->y2 = -sinw; }
static double osc_next(osc_t *o)
{
    double out = o->y1, nx = o->c * o->y1 - o->y2;
    o->y2 = o->y1; o->y1 = nx;
    return out;
}

static uint32_t rng;
static float noise(void)   /* dsp_test.c frand() */
{
    rng = rng * 1664525u + 1013904223u;
    return ((rng >> 8) & 0xffff) / 32768.0f - 1.0f;
}

/* vocal_process(): the mapping of the Wah value to the DSP target */
static void map_value(float v, int *eng, int invert, float *target, int *engaged)
{
    if (!(v >= -100.0f && v <= 100.0f)) v = 0.0f;           /* NaN / out of range */
    *engaged = *eng = vfx_engage(v, *eng);
    *target = *engaged ? v * 0.01f : 0.0f;
    if (invert) *target = -*target;
}

int main(int argc, char **argv)
{
    FILE *sc, *out;
    char line[256];
    vfx_coefs coefs;
    vfx_state st;
    osc_t o220 = {0}, o1000 = {0}, o60 = {0}, o3000 = {0}, o440 = {0};
    int invert = 0, eng = 0, started = 0;
    unsigned long total = 0;
    static float L[MAXBLK], R[MAXBLK];

    memset(&st, 0, sizeof st);
    if (argc != 3) { fprintf(stderr, "usage: vocal_ref scenario.txt out.f32\n"); return 2; }
    sc = fopen(argv[1], "r");
    out = fopen(argv[2], "wb");
    if (!sc || !out) { fprintf(stderr, "cannot open files\n"); return 2; }

    while (fgets(line, sizeof line, sc)) {
        float value, gain;
        unsigned frames, block, off;
        int silent, inject, pass_invert;
        if (line[0] == '#' || line[0] == '\n' || line[0] == '\r') continue;
        if (sscanf(line, "pass %d", &pass_invert) == 1) {
            /* a pass starts from rest: fresh DSP state, generators and engage decision */
            invert = pass_invert; eng = 0; started = 1;
            vfx_design(&coefs, 44100.0);
            vfx_reset(&st);
            rng = 12345u;
            osc_init(&o220, 1.9990175909339492, 0.03133955622145082);
            osc_init(&o1000, 1.9797349455598832, 0.14199431795762676);
            osc_init(&o60, 1.9999269227133345, 0.008548447320595277);
            osc_init(&o3000, 1.8200702223285332, 0.4145311766902954);
            osc_init(&o440, 1.99607132886337, 0.06264832417874368);
            continue;
        }
        if (sscanf(line, "%f %u %u %f %d %d", &value, &frames, &block, &gain, &silent, &inject) != 6 || !started || block == 0 || block > MAXBLK) {
            fprintf(stderr, "bad scenario line: %s", line);
            return 2;
        }
        for (off = 0; off < frames; off += block) {
            unsigned n = frames - off < block ? frames - off : block, i;
            float target;
            int engaged;
            for (i = 0; i < n; i++) {
                unsigned rel = off + i;
                double c = 0.2 * osc_next(&o220) + 0.15 * osc_next(&o1000) + 0.1 * osc_next(&o60) + 0.08 * osc_next(&o3000);
                double sd = 0.15 * (double)noise() + 0.1 * osc_next(&o440);
                float l = (float)(c + sd) * gain, r = (float)(c - sd) * gain;
                if (silent) { l = 0.0f; r = 0.0f; }
                if (inject) {            /* garbage samples the DSP has to contain */
                    if (rel == 10) l = NAN;
                    if (rel == 11) r = INFINITY;
                    if (rel == 12) l = -1e5f;
                    if (rel == 13) r = 1e30f;
                }
                L[i] = l; R[i] = r;
            }
            map_value(value, &eng, invert, &target, &engaged);
            vfx_process(&coefs, &st, L, R, n, target, engaged);
            for (i = 0; i < n; i++) {
                fwrite(&L[i], 4, 1, out);
                fwrite(&R[i], 4, 1, out);
            }
            total += n;
        }
    }
    fclose(sc);
    fclose(out);
    printf("%lu frames\n", total);
    return 0;
}
