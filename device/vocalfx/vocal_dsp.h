/* Vocal FX DSP: mid/side with a phase-coherent three-band split of the centre, portable C (libm only in vfx_design).
 *
 * Why not "band-pass the mid and subtract it": any band-pass has phase, so x - bandpass(x) only cancels near one
 * frequency (measured: about -2 dB on a voice, -7 dB at 1 kHz). Instead the mid signal is split by two Linkwitz-Riley
 * 4th-order crossovers (VFX_F1_HZ, VFX_F2_HZ) into low / mid / high bands that sum back to an all-pass of the input,
 * and the side signal goes through the same all-pass so the stereo image stays coherent. Gains are then applied per
 * band, which is exact and has no notches at partial settings:
 *
 *   focus (k = +kp):  low, high * (1-kp);  mid * (1 + kp*(G-1));  side * (1-kp)    -> at kp = 1 only the centre band, mono
 *   cut   (k = -kn):  mid * (1-kn); everything else unchanged                       -> at kn = 1 the centre band is gone
 *
 * k is smoothed per sample. An engage weight e crossfades from the untouched input to the processed signal (the
 * all-pass differs from the input in phase), and the block is left bit-identical while idle. A non-finite or absurd
 * input sample (VFX_SANE) is replaced by 0 on entry, so one bad sample cannot spread through the filters.
 * 11 biquads per stereo frame (float32, transposed direct form II): LP/HP at F1 and F2 as LR4 (two identical BW2
 * sections each) on mid, one all-pass on the low band, two all-passes on the side.
 */
#ifndef VOCAL_DSP_H
#define VOCAL_DSP_H

#define VFX_F1_HZ      150.0      /* low / mid crossover */
#define VFX_F2_HZ      4000.0     /* mid / high crossover */
#define VFX_SMOOTH_MS  10.0       /* one-pole time constant of the amount and of the engage weight */
#define VFX_FOCUS_GAIN 1.4f       /* make-up gain of the kept centre band at full focus (about +2.9 dB) */
#define VFX_DENORM_OFF 1e-18f     /* alternating offset into every chain: keeps all states normal numbers */
#define VFX_KNEE       0.97f      /* processed samples above this are compressed towards 1.0 (the all-pass raises crest factors); high on purpose:
                                   * below the knee the path is linear, so hot-mastered material at a near-neutral setting stays clean */
#define VFX_SANE       1e4f       /* an input sample at or beyond this (or NaN / Inf) is treated as garbage and replaced by 0 before it reaches a filter */
#define VFX_ENGAGE_ON  1.0f       /* |Wah value| at which the effect engages ... */
#define VFX_ENGAGE_OFF 0.5f       /* ... and below which it releases again: a pad value hovering around +-1 must not toggle the effect */
#define VFX_NBQ        11

/* the float build is the product; a double build (-DVFX_REAL=double -DVFX_SUFFIX=_d) is the accuracy reference in test/ */
#ifndef VFX_REAL
#define VFX_REAL float
#endif
#ifndef VFX_SUFFIX
#define VFX_SUFFIX
#endif
#define VFX_CAT2(a, b) a##b
#define VFX_CAT(a, b) VFX_CAT2(a, b)
#define vfx_design  VFX_CAT(vfx_design, VFX_SUFFIX)
#define vfx_reset   VFX_CAT(vfx_reset, VFX_SUFFIX)
#define vfx_process VFX_CAT(vfx_process, VFX_SUFFIX)
#define vfx_bq      VFX_CAT(vfx_bq, VFX_SUFFIX)
#define vfx_coefs   VFX_CAT(vfx_coefs, VFX_SUFFIX)
#define vfx_state   VFX_CAT(vfx_state, VFX_SUFFIX)
#define vfx_denorm_hits VFX_CAT(vfx_denorm_hits, VFX_SUFFIX)
typedef VFX_REAL vfx_real;

typedef struct { vfx_real b0, b1, b2, a1, a2; } vfx_bq;
/* lp1/hp1/ap1: F1 sections, lp2/hp2/ap2: F2 sections (LR4 = the same BW2 section applied twice) */
typedef struct { vfx_bq lp1, hp1, ap1, lp2, hp2, ap2; vfx_real alpha; } vfx_coefs;
typedef struct { vfx_real z1[VFX_NBQ], z2[VFX_NBQ]; vfx_real k, e, dn; int active; } vfx_state;

void vfx_design(vfx_coefs *c, double fs);
void vfx_reset(vfx_state *s);
/* In place on L/R (n frames). target: -1..+1 (positive = vocal focus), 0 together with |target| < 1e-4 means "off".
 * Returns 1 when the block was processed, 0 when it was left untouched (idle bypass). */
int vfx_process(const vfx_coefs *c, vfx_state *s, vfx_real *L, vfx_real *R, unsigned n, vfx_real target, int engaged);
/* Engage decision with hysteresis. v = Wah value -100..100 (finite), was = the previous decision. Header-only so the
 * tests exercise the product's own rule. */
static inline int vfx_engage(float v, int was)
{
    float a = v < 0.0f ? -v : v;
    return was ? (a >= VFX_ENGAGE_OFF) : (a >= VFX_ENGAGE_ON);
}
/* debug: number of intermediate values seen in the subnormal range (only counted with -DVFX_CHECK_DENORM) */
extern unsigned long vfx_denorm_hits;

#endif
