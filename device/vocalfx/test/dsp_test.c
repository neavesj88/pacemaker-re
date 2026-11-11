/* Native (x86) verification of vocal_dsp.c: band/side attenuation per setting, bypass exactness, block-size
 * independence, click-free amount changes, no NaN / subnormal / runaway output, float-vs-double accuracy,
 * and a speed benchmark. Build and run via ../test.sh. Exit status 1 on any failed check. */
#define _GNU_SOURCE
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>
#if defined(__x86_64__) || defined(__i386__)
#include <x86intrin.h>
#include <xmmintrin.h>
#include <pmmintrin.h>
#endif
#include "../vocal_dsp.h"

void ref_init(double fs);
int ref_process(double *L, double *R, unsigned n, double target, int engaged);

#define FS 44100.0
#define BLK 512
static int fails;
#define CHECK(cond, ...) do { if (!(cond)) { fails++; printf("  FAIL: "); printf(__VA_ARGS__); printf("\n"); } } while (0)

static uint32_t rng = 12345u;
static float frand(void) { rng = rng * 1664525u + 1013904223u; return ((rng >> 8) & 0xffff) / 32768.0f - 1.0f; } /* -1..1 */
static int finite_f(float x) { return x == x && fabsf(x) < 1e30f; }

/* the product's mapping: |Wah value| < 1 (target 0.01) is the neutral dead zone */
static int proc(const vfx_coefs *c, vfx_state *s, float *L, float *R, unsigned n, float target)
{
    return vfx_process(c, s, L, R, n, target, fabsf(target) >= 0.01f);
}

enum { VOICE, SIDE_NOISE, BASS60, MIX, TONE };
static const char *signame[] = { "centre voice 220Hz+harm", "side wideband noise", "centre bass 60Hz", "mix" };

static void gen(int kind, float *L, float *R, long n, long off, double hz)
{
    long i;
    for (i = 0; i < n; i++) {
        double t = (double)(off + i) / FS;
        float v = 0, nz = frand() * 0.35f, b = 0;
        int h;
        if (kind == VOICE || kind == MIX) for (h = 1; h <= 15; h++) v += (float)(0.12 / h * sin(2 * M_PI * 220.0 * h * t + h));
        if (kind == BASS60 || kind == MIX) b = (float)(0.45 * sin(2 * M_PI * 60.0 * t));
        if (kind == TONE) v = (float)(0.5 * sin(2 * M_PI * hz * t));
        L[i] = R[i] = 0;
        if (kind == VOICE || kind == TONE) { L[i] = R[i] = v; }
        if (kind == BASS60) { L[i] = R[i] = b; }
        if (kind == SIDE_NOISE) { L[i] = nz; R[i] = -nz; }
        if (kind == MIX) { L[i] = v + b + nz; R[i] = v + b - nz; }
    }
}

static double rms2(const float *L, const float *R, long n)
{
    double a = 0; long i;
    for (i = 0; i < n; i++) a += (double)L[i] * L[i] + (double)R[i] * R[i];
    return sqrt(a / (2.0 * n));
}

/* a signal through the DSP at a fixed target: output gain in dB after settling */
static double gain_db(const vfx_coefs *c, int kind, double hz, float target)
{
    long total = (long)(FS * 1.6), settle = (long)(FS * 0.6), i;
    float *L = malloc(total * 4), *R = malloc(total * 4), *iL = malloc(total * 4), *iR = malloc(total * 4);
    vfx_state s; double gi, go;
    gen(kind, L, R, total, 0, hz);
    memcpy(iL, L, total * 4); memcpy(iR, R, total * 4);
    vfx_reset(&s);
    for (i = 0; i < total; i += BLK) proc(c, &s, L + i, R + i, (unsigned)(i + BLK <= total ? BLK : total - i), target);
    gi = rms2(iL + settle, iR + settle, total - settle);
    go = rms2(L + settle, R + settle, total - settle);
    free(L); free(R); free(iL); free(iR);
    return go < 1e-12 ? -240.0 : 20 * log10(go / gi);
}

int main(void)
{
    vfx_coefs c; int si, ti;
    static const float targets[] = { -1.0f, -0.5f, -0.25f, 0.0f, 0.25f, 0.5f, 1.0f };
    static const double hzs[] = { 40, 60, 100, 150, 220, 300, 500, 1000, 2000, 3000, 4000, 6000, 10000, 16000 };
    enum { NH = 14 };
    vfx_design(&c, FS);
    printf("crossovers LR4 at %.0f / %.0f Hz, smoothing alpha=%.5f (%.1f ms), focus make-up %.2f\n",
           VFX_F1_HZ, VFX_F2_HZ, c.alpha, VFX_SMOOTH_MS, VFX_FOCUS_GAIN);

    printf("\n[1] output level vs input (dB RMS, settled); Wah value = target*100, + = vocal focus, - = vocal cut\n");
    printf("%-26s", "signal \\ Wah value");
    for (ti = 0; ti < 7; ti++) printf("%8.0f", targets[ti] * 100);
    printf("\n");
    double tab[4][7];
    for (si = 0; si < 4; si++) {
        printf("%-26s", signame[si]);
        for (ti = 0; ti < 7; ti++) { tab[si][ti] = gain_db(&c, si, 0, targets[ti]); printf("%8.1f", tab[si][ti]); }
        printf("\n");
    }
    printf("%-26s", "voice vs (side+bass) dB");
    for (ti = 0; ti < 7; ti++) {
        double v = tab[VOICE][ti] - 20 * log10(pow(10, tab[SIDE_NOISE][ti] / 20) + pow(10, tab[BASS60][ti] / 20) + 1e-12);
        printf("%+8.1f", v + 20 * log10(2.0));
    }
    printf("   (change vs neutral)\n");

    printf("\n[2] centre tone response (dB)\n%10s", "Hz");
    for (ti = 0; ti < NH; ti++) printf("%7.0f", hzs[ti]);
    printf("\n");
    double cutr[NH], focr[NH], half[NH];
    printf("%10s", "cut -100"); for (ti = 0; ti < NH; ti++) { cutr[ti] = gain_db(&c, TONE, hzs[ti], -1.0f); printf("%7.1f", cutr[ti]); }
    printf("\n%10s", "cut -50"); for (ti = 0; ti < NH; ti++) { half[ti] = gain_db(&c, TONE, hzs[ti], -0.5f); printf("%7.1f", half[ti]); }
    printf("\n%10s", "focus +50"); for (ti = 0; ti < NH; ti++) printf("%7.1f", gain_db(&c, TONE, hzs[ti], 0.5f));
    printf("\n%10s", "focus +100"); for (ti = 0; ti < NH; ti++) { focr[ti] = gain_db(&c, TONE, hzs[ti], 1.0f); printf("%7.1f", focr[ti]); }
    printf("\n");
    {   /* partial settings must not make notches: cut -50 is about -6 dB in the band, never deeper than -7 */
        double worst = 0; for (ti = 0; ti < NH; ti++) if (half[ti] < worst) worst = half[ti];
        CHECK(worst > -7.0, "cut -50 has a notch: %.1f dB", worst);
    }
    CHECK(fabs(tab[VOICE][3]) < 0.01, "neutral voice gain %.3f dB", tab[VOICE][3]);
    CHECK(tab[2][6] < -25, "focus +100 centre-bass attenuation only %.1f dB", tab[BASS60][6]);
    CHECK(tab[SIDE_NOISE][6] < -80, "focus +100 side attenuation only %.1f dB", tab[SIDE_NOISE][6]);
    CHECK(tab[VOICE][6] > 1.0 && tab[VOICE][6] < 4.0, "focus +100 voice gain %.1f dB outside 1..4", tab[VOICE][6]);
    CHECK(tab[VOICE][0] < -15, "cut -100 voice attenuation only %.1f dB", tab[VOICE][0]);
    CHECK(fabs(tab[SIDE_NOISE][0]) < 0.3, "cut changes side noise by %.3f dB (all-pass only)", tab[SIDE_NOISE][0]);
    CHECK(tab[BASS60][0] > -1.0, "cut -100 touches 60 Hz centre bass: %.1f dB", tab[BASS60][0]);
    CHECK(cutr[7] < -35, "cut at 1 kHz only %.1f dB", cutr[7]);
    CHECK(cutr[6] < -30 && cutr[8] < -20, "cut at 500 Hz / 2 kHz only %.1f / %.1f dB", cutr[6], cutr[8]);
    CHECK(tab[SIDE_NOISE][5] < -5.5 && tab[SIDE_NOISE][5] > -7.0, "focus +50 side %.1f dB (expect about -6)", tab[SIDE_NOISE][5]);
    (void)focr;

    printf("\n[3] idle bypass is bit-exact; engage then release returns to bit-exact bypass\n");
    {
        float L[BLK], R[BLK], oL[BLK], oR[BLK]; vfx_state s; int b, r;
        vfx_reset(&s);
        gen(MIX, L, R, BLK, 0, 0); memcpy(oL, L, sizeof L); memcpy(oR, R, sizeof R);
        r = proc(&c, &s, L, R, BLK, 0.0f);
        CHECK(r == 0 && !memcmp(L, oL, sizeof L) && !memcmp(R, oR, sizeof R), "idle block was modified or reported processed (r=%d)", r);
        for (b = 0; b < 40; b++) { gen(MIX, L, R, BLK, b * BLK, 0); proc(&c, &s, L, R, BLK, 1.0f); }
        for (b = 0, r = 1; b < 400 && r; b++) {
            gen(MIX, L, R, BLK, 0, 0); memcpy(oL, L, sizeof L); memcpy(oR, R, sizeof R);
            r = proc(&c, &s, L, R, BLK, 0.0f);
        }
        CHECK(r == 0 && !memcmp(L, oL, sizeof L) && !memcmp(R, oR, sizeof R), "did not return to bit-exact bypass after release (blocks=%d)", b);
        printf("  back to exact bypass %d blocks (%.0f ms) after release\n", b, b * BLK / FS * 1000);
    }

    printf("\n[4] call granularity does not change the output (constant target)\n");
    {
        static float A[20000], B[20000], A2[20000], B2[20000], A3[20000], B3[20000]; vfx_state s1, s2, s3; long i;
        gen(MIX, A, B, 20000, 0, 0);
        memcpy(A2, A, sizeof A); memcpy(B2, B, sizeof B); memcpy(A3, A, sizeof A); memcpy(B3, B, sizeof B);
        vfx_reset(&s1); vfx_reset(&s2); vfx_reset(&s3);
        for (i = 0; i < 20000; i++) proc(&c, &s1, A + i, B + i, 1, 0.7f);
        for (i = 0; i < 20000; i += 500) proc(&c, &s2, A2 + i, B2 + i, 500, 0.7f);
        proc(&c, &s3, A3, B3, 20000, 0.7f);
        CHECK(!memcmp(A, A2, sizeof A) && !memcmp(B, B2, sizeof B), "1-frame vs 500-frame calls differ");
        CHECK(!memcmp(A, A3, sizeof A) && !memcmp(B, B3, sizeof B), "1-frame vs 20000-frame call differ");
        printf("  1-, 500- and 20000-frame calls bit-identical: %s\n", (!memcmp(A, A2, sizeof A) && !memcmp(A, A3, sizeof A)) ? "yes" : "NO");
    }

    printf("\n[5] amount steps 0 -> +100 -> -100 -> 0 on a 1 kHz centre tone: no clicks\n");
    {
        long total = (long)(FS * 0.9), i; float *L = malloc(total * 4), *R = malloc(total * 4);
        vfx_state s; double maxd = 0, bound = 0.5 * 2 * M_PI * 1000.0 / FS * 1.45 * 1.5;   /* slope of a 0.5 sine with make-up and margin */
        gen(TONE, L, R, total, 0, 1000.0); vfx_reset(&s);
        for (i = 0; i < total; i += BLK) {
            float tg = (i < FS * 0.1) ? 0.0f : (i < FS * 0.4) ? 1.0f : (i < FS * 0.65) ? -1.0f : 0.0f;
            proc(&c, &s, L + i, R + i, (unsigned)(i + BLK <= total ? BLK : total - i), tg);
        }
        for (i = 1; i < total; i++) { double d = fabs(L[i] - L[i - 1]); if (d > maxd) maxd = d; }
        printf("  max |sample delta| = %.4f (sine slope bound %.4f)\n", maxd, bound);
        CHECK(maxd < bound, "discontinuity: %.4f > %.4f", maxd, bound);
        free(L); free(R);
    }

    printf("\n[6] float32 vs the same code in double precision (ideal coefficients): mix, three settings\n");
    {
        long total = (long)(FS * 2), i; int mode; static const float tg3[3] = { -1.0f, 0.5f, 1.0f };
        for (mode = 0; mode < 3; mode++) {
            float *L = malloc(total * 4), *R = malloc(total * 4); vfx_state s; double err = 0, sig = 0;
            double *dL = malloc(total * 8), *dR = malloc(total * 8);
            gen(MIX, L, R, total, 0, 0);
            for (i = 0; i < total; i++) { dL[i] = L[i]; dR[i] = R[i]; }
            vfx_reset(&s); ref_init(FS);
            for (i = 0; i < total; i += BLK) {
                unsigned n = (unsigned)(i + BLK <= total ? BLK : total - i);
                proc(&c, &s, L + i, R + i, n, tg3[mode]);
                ref_process(dL + i, dR + i, n, tg3[mode], 1);
            }
            for (i = 0; i < total; i++) { err += (L[i] - dL[i]) * (L[i] - dL[i]); sig += dL[i] * dL[i]; }
            printf("  Wah %+4.0f: error %.1f dB relative to output RMS\n", tg3[mode] * 100, 10 * log10(err / sig + 1e-30));
            CHECK(10 * log10(err / sig + 1e-30) < -70, "float vs double error too large");
            free(L); free(R); free(dL); free(dR);
        }
    }

    printf("\n[7] stress: full-scale noise, random targets, a NaN sample, then 10 s of silence (FTZ/DAZ off)\n");
    {
        vfx_state s; long blk, i; float L[BLK], R[BLK]; double peak_out = 0; int nonfinite = 0, tiny = 0; float tg = 0;
#if defined(__x86_64__) || defined(__i386__)
        _MM_SET_FLUSH_ZERO_MODE(_MM_FLUSH_ZERO_OFF); _MM_SET_DENORMALS_ZERO_MODE(_MM_DENORMALS_ZERO_OFF);
#endif
        vfx_denorm_hits = 0; vfx_reset(&s);
        for (blk = 0; blk < 4000; blk++) {
            if (blk % 7 == 0) { int r = (int)((frand() + 1.0f) * 2.0f); tg = (r == 0) ? 1.0f : (r == 1) ? -1.0f : (r == 2) ? 0.0f : frand(); }
            for (i = 0; i < BLK; i++) { L[i] = frand(); R[i] = frand(); }
            if (blk == 2000) L[100] = NAN;
            proc(&c, &s, L, R, BLK, tg);
            if (blk > 2010) for (i = 0; i < BLK; i++) {
                if (!finite_f(L[i]) || !finite_f(R[i])) nonfinite++;
                if (fabs(L[i]) > peak_out) peak_out = fabs(L[i]);
            }
        }
        printf("  after the NaN: %d non-finite output samples (blocks 2011..3999); peak |out| = %.3f for |in| <= 1\n", nonfinite, peak_out);
        CHECK(nonfinite == 0, "filter did not recover from NaN input (%d samples)", nonfinite);
        CHECK(peak_out < 3.0, "unbounded output %.2f", peak_out);
        for (blk = 0; blk < 50; blk++) { for (i = 0; i < BLK; i++) { L[i] = 0.5f * frand(); R[i] = L[i]; } proc(&c, &s, L, R, BLK, 1.0f); }
        vfx_denorm_hits = 0;
        for (blk = 0; blk < 860; blk++) {
            memset(L, 0, sizeof L); memset(R, 0, sizeof R);
            proc(&c, &s, L, R, BLK, blk < 300 ? 1.0f : -1.0f);
            for (i = 0; i < BLK; i++) if (L[i] != 0.0f && fabsf(L[i]) < 1e-30f) tiny++;
        }
        printf("  subnormal intermediate values during the silence tail: %lu (%s build); output samples under 1e-30: %d\n", vfx_denorm_hits,
#ifdef VFX_CHECK_DENORM
               "instrumented",
#else
               "NOT instrumented",
#endif
               tiny);
#ifdef VFX_CHECK_DENORM
        CHECK(vfx_denorm_hits == 0, "%lu subnormal intermediates", vfx_denorm_hits);
#endif
        CHECK(tiny == 0, "output samples in the tiny range");
    }

    printf("\n[8] synthetic mix (voice + side noise + bass) normalised to peak 0.9: output peak per setting\n  ");
    for (ti = 0; ti < 7; ti++) {
        long total = (long)(FS * 1.5), i; float *L = malloc(total * 4), *R = malloc(total * 4); vfx_state s; float pk = 0, o = 0;
        gen(MIX, L, R, total, 0, 0);
        for (i = 0; i < total; i++) { if (fabsf(L[i]) > pk) pk = fabsf(L[i]); if (fabsf(R[i]) > pk) pk = fabsf(R[i]); }
        for (i = 0; i < total; i++) { L[i] *= 0.9f / pk; R[i] *= 0.9f / pk; }
        vfx_reset(&s);
        for (i = 0; i < total; i += BLK) proc(&c, &s, L + i, R + i, (unsigned)(i + BLK <= total ? BLK : total - i), targets[ti]);
        for (i = (long)(FS * 0.5); i < total; i++) { if (fabsf(L[i]) > o) o = fabsf(L[i]); if (fabsf(R[i]) > o) o = fabsf(R[i]); }
        printf("%+.0f:%.3f  ", targets[ti] * 100, o);
        CHECK(o <= 1.0f, "mix peak %.3f exceeds full scale at setting %.0f", o, targets[ti] * 100);
        free(L); free(R);
    }
    printf("\n");

    printf("\n[8b] soft knee bounds the output at 1.0: 1.5-amplitude centre tone, Wah +30\n");
    {
        static float L[BLK], R[BLK]; vfx_state s; int i, b; double maxout = 0;
        vfx_reset(&s);
        for (b = 0; b < 60; b++) {
            for (i = 0; i < BLK; i++) { L[i] = R[i] = 1.5f * sinf((float)(2 * M_PI * 1000.0 * (b * BLK + i) / FS)); }
            proc(&c, &s, L, R, BLK, 0.3f);
            if (b > 20) for (i = 0; i < BLK; i++) if (fabs(L[i]) > maxout) maxout = fabs(L[i]);
        }
        printf("  output peak %.4f\n", maxout);
        CHECK(maxout <= 1.0, "output exceeds 1.0: %.4f", maxout);
    }

    printf("\n[8c] hot-mastered material at a near-neutral setting (Wah +2, 1 kHz + 3 kHz centre, worst of 12 phase offsets): distortion vs a linear path\n");
    {
        static const double peaks[] = { 0.80, 0.95, 0.989 };
        int pi, ph;
        for (pi = 0; pi < 3; pi++) {
            double worst = -1000, worst_opk = 0; int worst_ph = 0;
            for (ph = 0; ph < 12; ph++) {
                long total = (long)(FS * 1.2), settle = (long)(FS * 0.5), i; float *H = malloc(total * 4), *Hr = malloc(total * 4), *Q = malloc(total * 4), *Qr = malloc(total * 4);
                vfx_state sh, sq; double err = 0, sig = 0, opk = 0, db;
                for (i = 0; i < total; i++) {
                    double t = (double)i / FS, x = sin(2 * M_PI * 1000.0 * t) + 0.8 * sin(2 * M_PI * 3000.0 * t + ph * M_PI / 6);
                    H[i] = Hr[i] = (float)(x * peaks[pi] / 1.8); Q[i] = Qr[i] = 0.5f * H[i];   /* Q: same signal at half level (far below the knee) */
                }
                vfx_reset(&sh); vfx_reset(&sq);
                for (i = 0; i < total; i += BLK) {
                    unsigned n = (unsigned)(i + BLK <= total ? BLK : total - i);
                    proc(&c, &sh, H + i, Hr + i, n, 0.02f);        /* L and R carry the same samples: a centre signal */
                    proc(&c, &sq, Q + i, Qr + i, n, 0.02f);
                }
                for (i = settle; i < total; i++) { double d = H[i] - 2.0 * Q[i]; err += d * d; sig += (double)H[i] * H[i]; if (fabs(H[i]) > opk) opk = fabs(H[i]); }
                db = 10 * log10(err / sig + 1e-30);
                if (db > worst) { worst = db; worst_opk = opk; worst_ph = ph; }
                free(H); free(Hr); free(Q); free(Qr);
            }
            printf("  input peak %.3f: worst phase %d: output peak %.3f, nonlinear residue %.1f dB re output RMS\n", peaks[pi], worst_ph, worst_opk, worst);
            if (peaks[pi] < 0.9) CHECK(worst < -100, "nonlinearity at peak %.2f: %.1f dB", peaks[pi], worst);
            else CHECK(worst < -30, "nonlinearity at peak %.3f: %.1f dB (> -30)", peaks[pi], worst);
        }
    }


    printf("\n[8d] a bad input sample stays one sample: NaN, +Inf, -Inf, 1e30 in one stream (0.3 sine, Wah +50), compared with a clean run\n");
    {
        long total = 40 * BLK, i; float *A = malloc(total * 4), *B = malloc(total * 4), *Ar = malloc(total * 4), *Br = malloc(total * 4);
        vfx_state sa, sb; long nonfinite = 0, far = 0; double peak = 0;
        for (i = 0; i < total; i++) { A[i] = Ar[i] = B[i] = Br[i] = (float)(0.3 * sin(2 * M_PI * 1000.0 * i / FS)); }
        A[20 * BLK + 100] = NAN; Ar[20 * BLK + 300] = INFINITY; A[21 * BLK + 5] = -INFINITY; Ar[22 * BLK + 7] = 1e30f;
        vfx_reset(&sa); vfx_reset(&sb);
        for (i = 0; i < total; i += BLK) { proc(&c, &sa, A + i, Ar + i, BLK, 0.5f); proc(&c, &sb, B + i, Br + i, BLK, 0.5f); }
        for (i = 0; i < total; i++) {
            if (!finite_f(A[i]) || !finite_f(Ar[i])) nonfinite++;
            if (fabs(A[i]) > peak) peak = fabs(A[i]);
            if (i > 20 * BLK + 100 + 4096 && fabs(A[i] - B[i]) > 1e-3) far++;     /* the damage must have decayed 4096 samples after the last bad sample */
        }
        printf("  non-finite output samples: %ld; peak |out| %.3f; samples still differing from the clean run >1e-3 after 4096 samples: %ld\n", nonfinite, peak, far);
        CHECK(nonfinite == 0, "%ld non-finite output samples", nonfinite);
        CHECK(peak < 1.0, "peak %.3f", peak);
        CHECK(far == 0, "%ld samples still disturbed long after the bad ones", far);
        free(A); free(B); free(Ar); free(Br);
    }

    printf("\n[8e] engage hysteresis (vfx_engage): on at |v| >= %.1f, off below %.1f\n", VFX_ENGAGE_ON, VFX_ENGAGE_OFF);
    {
        int e = 0, toggles = 0, i, prev;
        static const float hover[] = { 0.9f, 1.1f, 0.95f, -1.05f, -0.9f, 1.0f, 0.99f, 0.6f, -0.7f };
        for (i = 0; i < 400; i++) { prev = e; e = vfx_engage(hover[i % 9], e); if (e != prev) toggles++; }
        printf("  hovering around +-1 for 400 blocks: %d toggle(s) (the old fixed dead zone toggled %d times)\n", toggles, 4 * 44);
        CHECK(toggles == 1 && e == 1, "hovering value toggled %d times / ended %d", toggles, e);
        prev = e; e = vfx_engage(0.4f, e); CHECK(prev == 1 && e == 0, "0.4 did not release");
        e = vfx_engage(0.99f, e); CHECK(e == 0, "0.99 engaged from idle");
        e = vfx_engage(-1.0f, e); CHECK(e == 1, "-1.0 did not engage");
        e = vfx_engage(-100.0f, 0); CHECK(e == 1, "-100 did not engage");
        e = vfx_engage(0.0f, 0); CHECK(e == 0, "0 engaged");
    }

    printf("\n[9] speed on this PC (not the device): 512-frame block, engaged\n");
    {
        static float L0[BLK], R0[BLK], L[BLK], R[BLK]; vfx_state s; int it, N = 20000; double best_ns = 1e18, tot_ns = 0; uint64_t best_cyc = ~0ull;
        gen(MIX, L0, R0, BLK, 0, 0); vfx_reset(&s);
        for (it = 0; it < N; it++) {
            struct timespec a, b; uint64_t c0 = 0, c1 = 0;
            memcpy(L, L0, sizeof L); memcpy(R, R0, sizeof R);
            clock_gettime(CLOCK_MONOTONIC, &a);
#if defined(__x86_64__) || defined(__i386__)
            c0 = __rdtsc();
#endif
            proc(&c, &s, L, R, BLK, (it & 1024) ? -0.9f : 0.9f);
#if defined(__x86_64__) || defined(__i386__)
            c1 = __rdtsc();
#endif
            clock_gettime(CLOCK_MONOTONIC, &b);
            double ns = (b.tv_sec - a.tv_sec) * 1e9 + (b.tv_nsec - a.tv_nsec);
            tot_ns += ns; if (ns < best_ns) best_ns = ns; if (c1 - c0 < best_cyc) best_cyc = c1 - c0;
        }
        printf("  best %.0f ns (%.2f ns/frame), mean %.0f ns, best %llu TSC ticks per block\n", best_ns, best_ns / BLK, tot_ns / N, (unsigned long long)best_cyc);
    }

    printf("\n%s (%d failed checks)\n", fails ? "DSP TEST FAILED" : "DSP TEST OK", fails);
    return fails ? 1 : 0;
}
