/* The same DSP source compiled in double precision: the accuracy reference for dsp_test.c. */
#define VFX_REAL double
#define VFX_SUFFIX _d
#include "../vocal_dsp.c"

static vfx_coefs rc;
static vfx_state rs;
void ref_init(double fs) { vfx_design(&rc, fs); vfx_reset(&rs); }
int ref_process(double *L, double *R, unsigned n, double target, int engaged) { return vfx_process(&rc, &rs, L, R, n, target, engaged); }
