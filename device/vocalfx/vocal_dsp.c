#include "vocal_dsp.h"
#include <math.h>
#define VFX_ABS(x) ((x) < 0 ? -(x) : (x))

unsigned long vfx_denorm_hits;

#ifdef VFX_CHECK_DENORM
#define DN(x) do { vfx_real _v = (x); if (_v != 0.0f && VFX_ABS(_v) < (sizeof(vfx_real) == 4 ? 1.17549435e-38 : 2.2250738585072014e-308)) vfx_denorm_hits++; } while (0)
#else
#define DN(x) ((void)0)
#endif

/* soft knee: identity up to VFX_KNEE, then asymptotic to 1.0 with continuous slope; never exceeds 1.0 for finite input */
static inline vfx_real soft(vfx_real x)
{
    vfx_real a = VFX_ABS(x);
    if (a > VFX_KNEE) {
        vfx_real u = (a - VFX_KNEE) * (1.0f / (1.0f - VFX_KNEE));
        a = VFX_KNEE + (1.0f - VFX_KNEE) * (u / (1.0f + u));
        return x < 0 ? -a : a;
    }
    return x;
}

enum { LP1A, LP1B, HP1A, HP1B, LP2A, LP2B, HP2A, HP2B, AP2L, AP1S, AP2S };

/* RBJ cookbook biquads, Butterworth Q = 1/sqrt2 */
static void bq_design(vfx_bq *q, int kind, double fs, double f0)
{
    const double Q = 0.70710678118654752;
    double w = 2.0 * M_PI * f0 / fs, cs = cos(w), al = sin(w) / (2.0 * Q);
    double a0 = 1.0 + al, b0, b1, b2;
    if (kind == 0)      { b0 = (1.0 - cs) / 2.0; b1 = 1.0 - cs;    b2 = b0; }          /* low-pass  */
    else if (kind == 1) { b0 = (1.0 + cs) / 2.0; b1 = -(1.0 + cs); b2 = b0; }          /* high-pass */
    else                { b0 = 1.0 - al;         b1 = -2.0 * cs;   b2 = 1.0 + al; }    /* all-pass  */
    q->b0 = (vfx_real)(b0 / a0); q->b1 = (vfx_real)(b1 / a0); q->b2 = (vfx_real)(b2 / a0);
    q->a1 = (vfx_real)(-2.0 * cs / a0); q->a2 = (vfx_real)((1.0 - al) / a0);
}

void vfx_design(vfx_coefs *c, double fs)
{
    bq_design(&c->lp1, 0, fs, VFX_F1_HZ);  bq_design(&c->hp1, 1, fs, VFX_F1_HZ);  bq_design(&c->ap1, 2, fs, VFX_F1_HZ);
    bq_design(&c->lp2, 0, fs, VFX_F2_HZ);  bq_design(&c->hp2, 1, fs, VFX_F2_HZ);  bq_design(&c->ap2, 2, fs, VFX_F2_HZ);
    c->alpha = (vfx_real)(1.0 - exp(-1.0 / (VFX_SMOOTH_MS * 1e-3 * fs)));
}

void vfx_reset(vfx_state *s)
{
    int i;
    for (i = 0; i < VFX_NBQ; i++) { s->z1[i] = 0.0f; s->z2[i] = 0.0f; }
    s->k = 0.0f; s->e = 0.0f; s->dn = VFX_DENORM_OFF; s->active = 0;
}

/* Biquads over a block, state and coefficients in registers; in == out is fine. Sections are run in groups of
 * independent chains: the ARM11 VFP is in-order with 8-cycle result latency, so three chains interleaved in one
 * loop keep the pipeline busy where a single biquad (z1 -> y -> z1 recurrence) would stall. The arithmetic per
 * section is the same as running them one after the other. */
typedef struct { const vfx_bq *q; vfx_real *z1, *z2; const vfx_real *in; vfx_real *out; } vfx_job;

static inline __attribute__((always_inline)) void bq_run3(vfx_job a, vfx_job b, vfx_job c, unsigned n)
{
    const vfx_real a0 = a.q->b0, a1 = a.q->b1, a2 = a.q->b2, a3 = a.q->a1, a4 = a.q->a2;
    const vfx_real b0 = b.q->b0, b1 = b.q->b1, b2 = b.q->b2, b3 = b.q->a1, b4 = b.q->a2;
    const vfx_real c0 = c.q->b0, c1 = c.q->b1, c2 = c.q->b2, c3 = c.q->a1, c4 = c.q->a2;
    vfx_real za1 = *a.z1, za2 = *a.z2, zb1 = *b.z1, zb2 = *b.z2, zc1 = *c.z1, zc2 = *c.z2;
    unsigned i;
    for (i = 0; i < n; i++) {
        vfx_real xa = a.in[i], xb = b.in[i], xc = c.in[i];
        vfx_real ya = a0 * xa + za1, yb = b0 * xb + zb1, yc = c0 * xc + zc1;
        za1 = a1 * xa - a3 * ya + za2; zb1 = b1 * xb - b3 * yb + zb2; zc1 = c1 * xc - c3 * yc + zc2;
        za2 = a2 * xa - a4 * ya;       zb2 = b2 * xb - b4 * yb;       zc2 = c2 * xc - c4 * yc;
        DN(ya); DN(yb); DN(yc); DN(za1); DN(zb1); DN(zc1); DN(za2); DN(zb2); DN(zc2);
        a.out[i] = ya; b.out[i] = yb; c.out[i] = yc;
    }
    *a.z1 = za1; *a.z2 = za2; *b.z1 = zb1; *b.z2 = zb2; *c.z1 = zc1; *c.z2 = zc2;
}

static inline __attribute__((always_inline)) void bq_run2(vfx_job a, vfx_job b, unsigned n)
{
    const vfx_real a0 = a.q->b0, a1 = a.q->b1, a2 = a.q->b2, a3 = a.q->a1, a4 = a.q->a2;
    const vfx_real b0 = b.q->b0, b1 = b.q->b1, b2 = b.q->b2, b3 = b.q->a1, b4 = b.q->a2;
    vfx_real za1 = *a.z1, za2 = *a.z2, zb1 = *b.z1, zb2 = *b.z2;
    unsigned i;
    for (i = 0; i < n; i++) {
        vfx_real xa = a.in[i], xb = b.in[i];
        vfx_real ya = a0 * xa + za1, yb = b0 * xb + zb1;
        za1 = a1 * xa - a3 * ya + za2; zb1 = b1 * xb - b3 * yb + zb2;
        za2 = a2 * xa - a4 * ya;       zb2 = b2 * xb - b4 * yb;
        DN(ya); DN(yb); DN(za1); DN(zb1); DN(za2); DN(zb2);
        a.out[i] = ya; b.out[i] = yb;
    }
    *a.z1 = za1; *a.z2 = za2; *b.z1 = zb1; *b.z2 = zb2;
}

#define J(Q, K, IN, OUT) ((vfx_job){ &c->Q, &z1[K], &z2[K], (IN), (OUT) })
#define VFX_SUB 128               /* frames per pass: 7 scratch arrays = 3.5 KB of stack, no allocation */

int vfx_process(const vfx_coefs *c, vfx_state *s, vfx_real *L, vfx_real *R, unsigned n, vfx_real target, int engaged)
{
    unsigned i, off;
    vfx_real k = s->k, e = s->e, al = c->alpha, dn = s->dn, et;
    vfx_real z1[VFX_NBQ], z2[VFX_NBQ];
    int q, bad;

    if (!(target == target)) target = 0.0f;               /* NaN guard */
    if (target > 1.0f) target = 1.0f; else if (target < -1.0f) target = -1.0f;
    et = engaged ? 1.0f : 0.0f;
    if (!engaged) target = 0.0f;

    if (!engaged && e < 1e-4f) {                          /* idle: leave the block alone, forget the state */
        if (s->active) vfx_reset(s);
        return 0;
    }
    if (!s->active) { vfx_reset(s); k = 0.0f; e = 0.0f; dn = s->dn; s->active = 1; }   /* cold start, fade in from the dry signal */
    for (q = 0; q < VFX_NBQ; q++) { z1[q] = s->z1[q]; z2[q] = s->z2[q]; }

    for (off = 0; off < n; off += VFX_SUB) {
        unsigned cnt = n - off < VFX_SUB ? n - off : VFX_SUB;
        vfx_real *Lp = L + off, *Rp = R + off;
        vfx_real xm[VFX_SUB], xs[VFX_SUB], lo[VFX_SUB], t1[VFX_SUB], mid[VFX_SUB], hi[VFX_SUB], side[VFX_SUB];

        for (i = 0; i < cnt; i++) {
            vfx_real m = 0.5f * (Lp[i] + Rp[i]), sd = 0.5f * (Lp[i] - Rp[i]);
            xm[i] = m + dn; xs[i] = sd - dn;                  /* alternating Nyquist offset, see VFX_DENORM_OFF */
            dn = -dn;
        }
        /* low / high split at F1, then the high part split at F2 (each LR4 = the same BW2 section twice) */
        bq_run3(J(lp1, LP1A, xm, lo),  J(hp1, HP1A, xm, t1),  J(ap1, AP1S, xs, side), cnt);   /* level 1 */
        bq_run3(J(lp1, LP1B, lo, lo),  J(hp1, HP1B, t1, t1),  J(ap2, AP2S, side, side), cnt); /* level 2 */
        bq_run3(J(ap2, AP2L, lo, lo),  J(hp2, HP2A, t1, hi),  J(lp2, LP2A, t1, mid), cnt);    /* level 3: low band onto the same all-pass as mid + high */
        bq_run2(J(hp2, HP2B, hi, hi),  J(lp2, LP2B, mid, mid), cnt);                          /* level 4 */

        for (i = 0; i < cnt; i++) {
            vfx_real l = Lp[i], r = Rp[i], kp, kn, gl, gm, mo, so;
            k += (target - k) * al;                           /* amount smoothing: no zipper noise */
            e += (et - e) * al;
            kp = k > 0.0f ? k : 0.0f;
            kn = k < 0.0f ? -k : 0.0f;
            gl = 1.0f - kp;                                   /* low and high bands, and the side */
            gm = (1.0f + kp * (VFX_FOCUS_GAIN - 1.0f)) * (1.0f - kn);
            mo = gl * (lo[i] + hi[i]) + gm * mid[i];
            so = gl * side[i];
            DN(mo); DN(so);
            Lp[i] = (1.0f - e) * l + e * soft(mo + so);
            Rp[i] = (1.0f - e) * r + e * soft(mo - so);
        }
    }

    /* state sanity: a NaN/Inf or runaway state (bad input) must not stick for ever */
    bad = !(VFX_ABS(k) <= 1.0001f && e >= -0.0001f && e <= 1.0001f);
    for (q = 0; q < VFX_NBQ; q++) if (!(VFX_ABS(z1[q]) < 1e6f && VFX_ABS(z2[q]) < 1e6f)) bad = 1;
    if (bad) { vfx_reset(s); return 1; }
    for (q = 0; q < VFX_NBQ; q++) { s->z1[q] = z1[q]; s->z2[q] = z2[q]; }
    s->k = k; s->e = e; s->dn = dn;
    return 1;
}
