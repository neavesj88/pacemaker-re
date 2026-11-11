/* libvocalfx.so: LD_PRELOAD hook that turns the Pacemaker's "Wah" FX (PeqAudioProcess) into a vocal
 * focus / vocal cut filter. See README.md and notes/vocal-fx-device.md.
 *
 * Target: firmware 180096069 pmplayer (ET_EXEC, fixed addresses). The only patch is ONE data word, the
 * `process` slot of the PeqAudioProcess vtable; it is replaced only after the stock value, the vtable
 * symbol, the process symbol and the two destructor slots all match. Anything else: do nothing.
 */
#ifndef _GNU_SOURCE
#define _GNU_SOURCE
#endif
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <pthread.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/time.h>
#include <time.h>
#include <unistd.h>
#include "vocal_dsp.h"

#define VFX_VERSION "0.1.0"

#ifndef VFX_DIR
#define VFX_DIR "/mnt/public/.Pacemaker"
#endif
#define FLAG_PATH VFX_DIR "/revival-vocal"
#define LOG_PATH  VFX_DIR "/revival-vocal.log"

/* ---- firmware 180096069 constants (all checked at load time) ---- */
#define SYM_VTABLE "_ZTVN7xponaut11audioengine15PeqAudioProcessE"
#define SYM_PROCESS "_ZN7xponaut11audioengine15PeqAudioProcess7processERKNS0_18AudioProcessBufferERS2_RKN4juce10MidiBufferERNS0_11EventBufferE"
#define SYM_D1 "_ZN7xponaut11audioengine15PeqAudioProcessD1Ev"
#define SYM_D0 "_ZN7xponaut11audioengine15PeqAudioProcessD0Ev"
#define SYM_RATE "_ZN7xponaut11audioengine12AudioProcess12m_sampleRateE"
#define VTABLE_ADDR 0x877e48u
#define SLOT_OFF    0x1cu          /* vtable + 0x1c = vptr[0x14], vptr = vtable + 8 */
#define SLOT_ADDR   0x877e64u
#define PROCESS_ADDR 0x4f524cu
#define D1_ADDR     0x4f5730u
#define D0_ADDR     0x4f58d8u

/* AudioProcessBuffer (notes/vocal-fx-device.md section 3) */
#define B_SILENT  0x04u
#define B_CH0     0x08u            /* float* per channel, stride 4 */
#define B_NCH     0x34u
#define B_FRAMES  0x38u
/* PeqAudioProcess */
#define P_BYPASS  0x04u            /* byte: stock process does nothing when set */
#define P_LASTVAL 0x80u            /* float written at the end of process(): getValue(true), -100..+100 */

#define MAXFRAMES 4096u
#define MAXINST   8
/* Timing is the only syscall in the audio path (gettimeofday, 2-3 per timed call): every call for the first
 * TIMED_FIRST calls, then 1 in 64. Build with -DVFX_TIMING=0 to remove it entirely. */
#ifndef VFX_TIMING
#define VFX_TIMING 1
#endif
#define TIMED_FIRST 64u
#define TIMED_MASK  63u

enum { ST_NOTLOADED = 0, ST_INSTALLED = 1, ST_NOT_PLAYER = -1, ST_NO_SYMS = -2, ST_MISMATCH = -3, ST_MPROTECT = -4 };

typedef void (*process_fn)(void *self, void *in, void *out, void *midi, void *events);

static process_fn g_orig;
static int g_status = ST_NOTLOADED;
static int g_dsp;                   /* flag file present at load time */
static int g_invert;                /* flag file starts with 'i' ("invert"): negative = focus */
static const char *g_why = "";
static uint32_t g_slot_before, g_slot_after;
static char g_exe[512];

static vfx_coefs g_coef;
static int g_coef_ready;
static const double *g_rate_p;      /* AudioProcess::m_sampleRate, resolved at load (no dlsym in the audio thread) */
static struct { void *self; uint32_t last; vfx_state st; } g_inst[MAXINST];

/* ---- stats: single writer (audio thread), read racily by the log thread; benign ---- */
static struct {
    volatile uint32_t calls, dsp_blocks, bypass_blocks, passthru_stock, nonalias, silent, bad_buf, recycled;
    volatile uint32_t timed, orig_us_sum, dsp_us_sum, tot_us_sum, orig_us_max, dsp_us_max, tot_us_max;
    volatile uint32_t ring_tot[16], ring_dsp[16];
    volatile float last_v, max_abs_v;
    volatile uint32_t last_frames, ninst;
} st;

static inline uint32_t now_us(void)
{
    struct timeval tv;
    gettimeofday(&tv, 0);
    return (uint32_t)((uint64_t)tv.tv_sec * 1000000u + (uint32_t)tv.tv_usec);
}

#if defined(__arm__)
static inline uint32_t fpscr_get(void) { uint32_t r; __asm__ volatile("vmrs %0, fpscr" : "=r"(r) : : "memory"); return r; }
static inline void fpscr_set(uint32_t r) { __asm__ volatile("vmsr fpscr, %0" : : "r"(r) : "memory"); }
#define FPSCR_FZ (1u << 24)        /* flush-to-zero: no denormal bounces to VFP support code */
#else
static inline uint32_t fpscr_get(void) { return 0; }
static inline void fpscr_set(uint32_t r) { (void)r; }
#define FPSCR_FZ 0u
#endif

/* One DSP state per PeqAudioProcess instance (two live decks, plus whatever mix playback creates). A table of
 * MAXINST slots keyed by the object address; when it is full the least recently used slot is recycled (a
 * destroyed instance is never called again, so it is the oldest). Allocation-free. */
static vfx_state *find_inst(void *self, uint32_t now)
{
    unsigned i, victim = 0;
    for (i = 0; i < MAXINST; i++) if (g_inst[i].self == self) { g_inst[i].last = now; return &g_inst[i].st; }
    for (i = 0; i < MAXINST; i++) {
        if (!g_inst[i].self) { victim = i; break; }
        if ((int32_t)(g_inst[i].last - g_inst[victim].last) < 0) victim = i;
    }
    vfx_reset(&g_inst[victim].st);
    g_inst[victim].self = self;
    g_inst[victim].last = now;
    if (st.ninst < MAXINST) st.ninst++; else st.recycled++;
    return &g_inst[victim].st;
}

static void __attribute__((noinline)) run_dsp(vfx_state *s, float *L, float *R, unsigned n, float target, int engaged, int *processed)
{
    *processed = vfx_process(&g_coef, s, L, R, n, target, engaged);
}

static void vocal_process(void *self, void *in, void *out, void *midi, void *events)
{
    uint8_t *ib = (uint8_t *)in, *ob = (uint8_t *)out, *sp = (uint8_t *)self;
    uint32_t n = ++st.calls, t0 = 0, t1 = 0, t2 = 0;
    int timed = VFX_TIMING && ((n <= TIMED_FIRST) || ((n & TIMED_MASK) == 0));
    vfx_state *inst;
    uint8_t was;
    float v, target;
    uint32_t frames, nch, fpscr;
    float *L, *R;
    int processed = 0, engaged;

    if (timed) t0 = now_us();

    /* Flag absent: pure observer, the stock Wah runs untouched. in != out (not the live deck chain):
     * also stock, the in == out layout is all we know. */
    if (!g_dsp || ib != ob) {
        if (ib != ob) st.nonalias++; else st.passthru_stock++;
        g_orig(self, in, out, midi, events);
        goto done;
    }
    if (!g_coef_ready) {            /* sample rate is a double the engine sets; sanity-check, else 44100 */
        double fs = 44100.0;
        const double *p = g_rate_p;
        if (p && *p >= 8000.0 && *p <= 192000.0) fs = *p;
        vfx_design(&g_coef, fs);
        g_coef_ready = 1;
    }
    inst = find_inst(self, n);

    /* Event pump: with the shared buffer flagged silent the stock process() still consumes every
     * parameter event and stores the value at self+0x80, but skips its filter (decompile of 0x4f524c;
     * copyFrom(x,x) is a no-op). Restore the flag afterwards. */
    was = ib[B_SILENT];
    ib[B_SILENT] = 1;
    g_orig(self, in, out, midi, events);
    ib[B_SILENT] = was;
    if (timed) t1 = now_us();

    if (was || sp[P_BYPASS]) {          /* no audio (stopped deck) or the stock FX is switched off: drop our state too */
        st.silent++;
        vfx_reset(inst);
        goto done;
    }

    { union { uint32_t u; float f; } cv; cv.u = *(volatile uint32_t *)(sp + P_LASTVAL); v = cv.f; }
    if (!(v >= -100.0f && v <= 100.0f)) v = 0.0f;           /* NaN / out of range */
    st.last_v = v;
    { float a = v < 0 ? -v : v; if (a > st.max_abs_v) st.max_abs_v = a; }
    engaged = !(v > -1.0f && v < 1.0f);                    /* +-1 dead zone around the neutral position */
    target = engaged ? v * 0.01f : 0.0f;
    if (g_invert) target = -target;

    frames = *(uint32_t *)(ob + B_FRAMES);
    nch = *(uint32_t *)(ob + B_NCH);
    L = *(float **)(ob + B_CH0);
    R = *(float **)(ob + B_CH0 + 4);
    st.last_frames = frames;
    if (frames == 0 || frames > MAXFRAMES || nch < 2 || !L || !R || L == R || ((uintptr_t)L & 3) || ((uintptr_t)R & 3)) {
        st.bad_buf++;
        goto done;
    }

    fpscr = fpscr_get();
    fpscr_set(fpscr | FPSCR_FZ);
    run_dsp(inst, L, R, frames, target, engaged, &processed);
    fpscr_set(fpscr);
    if (processed) st.dsp_blocks++; else st.bypass_blocks++;
    if (timed) t2 = now_us();

done:
    if (timed) {
        uint32_t end = t2 ? t2 : now_us(), tot = end - t0, o = t1 ? t1 - t0 : tot, d = t2 ? t2 - t1 : 0;
        uint32_t k = st.timed++ & 15u;
        st.orig_us_sum += o; st.dsp_us_sum += d; st.tot_us_sum += tot;
        if (o > st.orig_us_max) st.orig_us_max = o;
        if (d > st.dsp_us_max) st.dsp_us_max = d;
        if (tot > st.tot_us_max) st.tot_us_max = tot;
        st.ring_tot[k] = tot; st.ring_dsp[k] = d;
    }
}

/* ------------------------------------------------------------------------------------------------
 * log (never from the audio thread): header at load, then a thread rewrites the file every few seconds
 * ---------------------------------------------------------------------------------------------- */
/* no integer divide on ARMv6 (would need libgcc): use the VFP */
static uint32_t avg(uint32_t sum, uint32_t n) { return (uint32_t)((double)sum / (double)n); }

static void log_write(const char *tag, unsigned secs)
{
    char buf[2400];
    int n, fd, i;
    uint32_t t = st.timed ? st.timed : 1, last = st.timed < 16 ? st.timed : 16;
    n = snprintf(buf, sizeof buf,
        "libvocalfx %s [%s] t=+%us\n"
        "exe=%s status=%d%s%s\n"
        "slot 0x%08x: %08x -> %08x  dsp=%s invert=%d\n"
        "calls=%u dsp_blocks=%u bypass_blocks=%u stock_passthru=%u nonalias=%u silent=%u bad_buf=%u inst=%u recycled=%u\n"
        "param last=%.2f max_abs=%.2f frames=%u\n"
        "timing (sampled, %u calls timed; us per call incl. the stock process): total avg=%u max=%u | dsp avg=%u max=%u | stock avg=%u max=%u\n"
        "last timed totals(us):",
        VFX_VERSION, tag, secs, g_exe, g_status, g_why[0] ? " why=" : "", g_why,
        SLOT_ADDR, g_slot_before, g_slot_after, g_dsp ? "ON" : "off(flag absent)", g_invert,
        st.calls, st.dsp_blocks, st.bypass_blocks, st.passthru_stock, st.nonalias, st.silent, st.bad_buf, st.ninst, st.recycled,
        (double)st.last_v, (double)st.max_abs_v, st.last_frames,
        st.timed, avg(st.tot_us_sum, t), st.tot_us_max, avg(st.dsp_us_sum, t), st.dsp_us_max, avg(st.orig_us_sum, t), st.orig_us_max);
    for (i = 0; i < (int)last && n < (int)sizeof buf - 12; i++) n += snprintf(buf + n, sizeof buf - n, " %u", st.ring_tot[i]);
    if (n < (int)sizeof buf - 2) n += snprintf(buf + n, sizeof buf - n, "\n");
    fd = open(LOG_PATH, O_WRONLY | O_CREAT | O_TRUNC, 0644);
    if (fd >= 0) { ssize_t w = write(fd, buf, (size_t)n); (void)w; close(fd); }
}

static void *log_thread(void *arg)
{
    unsigned secs = 0, last_calls = 0, period = 5;
    (void)arg;
    for (;;) {
        struct timespec ts = { period, 0 };
        nanosleep(&ts, 0);
        secs += period;
        if (secs >= 300) period = 60;               /* settle to once a minute after five minutes */
        if (st.calls != last_calls) { last_calls = st.calls; log_write("run", secs); }
    }
    return 0;
}

static int is_player(void)
{
    ssize_t n = readlink("/proc/self/exe", g_exe, sizeof g_exe - 1);
    const char *b;
    if (n <= 0) return 0;
    g_exe[n] = 0;
    b = strrchr(g_exe, '/');
    b = b ? b + 1 : g_exe;
    return strcmp(b, "pmplayer.real") == 0;
}

__attribute__((constructor)) static void vfx_init(void)
{
    uint32_t *vt, *slot;
    void *fproc;
    long pg = 4096;
    uintptr_t page;
    pthread_t th;
    pthread_attr_t at;

    if (!is_player()) { g_status = ST_NOT_PLAYER; return; }

    vt = (uint32_t *)dlsym(RTLD_DEFAULT, SYM_VTABLE);
    fproc = dlsym(RTLD_DEFAULT, SYM_PROCESS);
    if (!vt || !fproc) { g_status = ST_NO_SYMS; g_why = "symbols not found"; goto report; }
    slot = (uint32_t *)((char *)vt + SLOT_OFF);
    g_slot_before = *slot;
    if ((uintptr_t)vt != VTABLE_ADDR || (uintptr_t)slot != SLOT_ADDR || *slot != PROCESS_ADDR ||
        (uintptr_t)fproc != PROCESS_ADDR || vt[2] != D1_ADDR || vt[3] != D0_ADDR ||
        (uintptr_t)dlsym(RTLD_DEFAULT, SYM_D1) != D1_ADDR || (uintptr_t)dlsym(RTLD_DEFAULT, SYM_D0) != D0_ADDR ||
        ((uintptr_t)&vocal_process & 1u)) {
        g_status = ST_MISMATCH; g_why = "stock bytes differ (other firmware?), nothing patched";
        goto report;
    }

    g_rate_p = (const double *)dlsym(RTLD_DEFAULT, SYM_RATE);

    {   /* the flag decides whether the DSP runs; read once, here */
        int fd = open(FLAG_PATH, O_RDONLY);
        if (fd >= 0) { char c = 0; g_dsp = 1; if (read(fd, &c, 1) == 1 && (c == 'i' || c == 'I')) g_invert = 1; close(fd); }
    }

    g_orig = (process_fn)(uintptr_t)*slot;
    page = (uintptr_t)slot & ~(uintptr_t)(pg - 1);
    if (mprotect((void *)page, (size_t)pg, PROT_READ | PROT_WRITE | PROT_EXEC) != 0) {   /* page also holds code: keep EXEC */
        g_status = ST_MPROTECT; g_why = "mprotect rw failed"; goto report;
    }
    *slot = (uint32_t)(uintptr_t)&vocal_process;
    g_slot_after = *slot;
    mprotect((void *)page, (size_t)pg, PROT_READ | PROT_EXEC);
    g_status = ST_INSTALLED;
    unsetenv("LD_PRELOAD");                          /* children of the player start clean */

report:
    log_write("load", 0);
    if (g_status != ST_INSTALLED) return;
    if (pthread_attr_init(&at) == 0) {
        pthread_attr_setstacksize(&at, 64 * 1024);
        if (pthread_create(&th, &at, log_thread, 0) == 0) pthread_detach(th);
        pthread_attr_destroy(&at);
    }
}

/* introspection for the dlopen test */
__attribute__((visibility("default"))) const char *vocalfx_version(void) { return VFX_VERSION; }
__attribute__((visibility("default"))) int vocalfx_status(void) { return g_status; }
