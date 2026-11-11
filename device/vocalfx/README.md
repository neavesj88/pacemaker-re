# libvocalfx: the "Vocal" FX in the Wah slot

An `LD_PRELOAD` library for the Tonium Pacemaker (i.MX31 ARM11, Linux 2.6.19.2, glibc 2.4, firmware 180096069/72).
It replaces one data word, the `process` slot of the `PeqAudioProcess` vtable ("Wah"), and turns the Wah pad into a
vocal filter. Research and hook plan: `notes/vocal-fx-device.md`; the preload mechanism: `notes/favourite-button.md`.
Nothing here has run on a device yet. Everything below marked "verified" was run on the PC (x86 DSP tests, the ARM
`.so` under qemu-user with the device's own `ld-2.4.so` and `libc-2.4.so`).

## What the pad does

The Wah value (-100..+100, centre 0 = off, pad is a rotary "donut") is mapped to one amount:

| Wah value | effect |
|---|---|
| inside -1..+1 | off, the audio is not touched (bit-identical). Engages at \|value\| >= 1 and releases only below 0.5 (hysteresis), so a pad hovering around ±1 does not toggle it |
| positive | **vocal focus**: low and high bands and the side signal fade out, the centre band (crossovers at 150 Hz and 4 kHz, both -6 dB points of the mid signal) is boosted up to +2.9 dB. At +100 only the centre band is left, mono |
| negative | **vocal cut**: the centre band is faded out, everything else is unchanged. At -100: -48 dB at 1 kHz, more than 25 dB only between about 300 Hz and 2 kHz, -15 dB at 220 Hz, -13 dB at 3 kHz, -6 dB at the 150 Hz and 4 kHz crossover points (table under DSP). Male fundamentals near 100-200 Hz and sibilance near 4 kHz are barely touched |

If the flag file starts with the letter `i` (see below) the two directions swap.

It is a centre-channel trick, not source separation: it works on vocals mixed dead centre (most are), fails on
wide, doubled or reverb-heavy ones, and removes other centred material in the band (snare, mono stabs) with them.

## Files

| file | what |
|---|---|
| `vocal_dsp.c/.h` | the DSP, portable C, float32, no libc except `libm` in `vfx_design` |
| `vocalfx.c` | the hook: constructor, patch, replacement `process`, stats, log thread |
| `build.sh` | cross-build + audit (WSL Debian) -> `work/build/vocalfx/libvocalfx.so` |
| `test.sh`, `test/` | native DSP test and benchmark, qemu-user tests with the device libc, wrapper snippet test |
| `wrapper-snippet.sh` | text for the `/bin/pmplayer` wrapper |
| `lang/english.lang`, `lang/japanese.lang` | stock files with string 236 `"Wah"` -> `"Vocal"` (only that line differs) |

## Build and test (WSL Debian, root)

```
apt-get install -y gcc libc6-dev gcc-arm-linux-gnueabi qemu-user-static python3     # once
wsl.exe -d Debian -u root bash device/vocalfx/build.sh      # cross-build + symbol-version audit
wsl.exe -d Debian -u root bash device/vocalfx/test.sh       # DSP tests, wrapper snippet, ARM under qemu
```

`build.sh` finds the firmware tree (`work/rootfs-180096069`, gitignored) by walking up from the checkout; set
`ROOTFS=...` if it lives elsewhere. Flags: `-march=armv6 -mtune=arm1136jf-s -mfpu=vfp -mfloat-abi=softfp -marm -fPIC -O2`,
no stack protector, `-funwind-tables`, `-U_TIME_BITS -U_FILE_OFFSET_BITS` (Debian's
compiler defines both as 64, which would reference `*64` time symbols glibc 2.4 does not have). Link is
`-nostdlib` against the **device's** `libpthread.so.0`, `libdl.so.2`, `libm.so.6`, `libgcc_s.so.1`, `libc.so.6`, `--hash-style=sysv`,
`-z now`. `e_flags` is set to the device libraries' `0x04000002` (EABI v4; ld.so 2.4 ignores it). The audit step
checks every undefined symbol against the version definitions in the device libs: 21 are `GLIBC_2.4`, plus the two
unwind personality routines `__aeabi_unwind_cpp_pr0/1@GCC_3.5` from `libgcc_s.so.1` (already loaded by the player; its
NEEDED list has it). The library carries `.ARM.exidx` (`-funwind-tables`) so that a C++ exception thrown inside the stock
`process()` unwinds through `vocal_process` instead of hitting a frame with no unwind info (which would terminate the
player); the audit rejects any other `GCC_*` symbol. No `libstdc++`. The build is reproducible (same sha256 from two runs
and from a different output directory).

Output: `libvocalfx.so`, 13,388 bytes; `build-info.txt` has the readelf summary.

## Install on the device

1. Add `libvocalfx.so` to the image as `/pmds/bin/libvocalfx.so` (mode 755), next to `pmplayer.real`.
2. The text of `wrapper-snippet.sh` goes into the `/bin/pmplayer` wrapper just before the final `exec`. Since firmware
   180096073 it is already inside `scripts/device/pmplayer-wrapper.sh` (verbatim, checked by `scripts/device/test-dma-wrapper.sh`),
   which `scripts/build-pfw.sh --diag` installs, so there is only one wrapper. The block sets
   `LD_PRELOAD` only if the library exists **and** `/mnt/public/.Pacemaker/revival-no-preload` does not. That empty
   file is the kill switch: create it from the PC over USB and the next boot is stock.
3. Replace `/pmds/bin/resources/fonts/english.lang` (and `japanese.lang`) with the files in `lang/`. The pad then
   reads "Vocal" where it said "Wah". The rootfs copy `usr/local/share/fonts/english.lang` is identical to the image's
   stock file; whether `pwrApplication` ever shows an FX name was not checked, and the Wah string is only an FX label.
4. To turn the DSP on, create `/mnt/public/.Pacemaker/revival-vocal` (empty file; content starting with `i` inverts
   the directions). It is read **once, at player start**, so reboot after creating or deleting it.
5. Optional, for the measurement boot only: create `/mnt/public/.Pacemaker/revival-vocal-timing` (empty file, also read
   once at start). It switches on the per-call timing in the log, which costs a `gettimeofday` syscall on a few calls.
   Delete it afterwards; without it the audio path makes no syscall at all.

With the flag file absent the library is a pure observer: the stock Wah runs unchanged and only the log is written.
That is the intended first boot.

## What the library does at load (all checked, otherwise it does nothing)

Runs only inside an executable named `pmplayer.real` (`/proc/self/exe`). Requires: `dlsym` of the vtable
`_ZTVN7xponaut11audioengine15PeqAudioProcessE` equals 0x877e48, the `process` symbol equals 0x4f524c, the slot at
0x877e64 holds 0x4f524c, the two destructor slots (vtable +8 / +12) equal the destructor symbols 0x4f5730 / 0x4f58d8.
Then `mprotect` the page read/write, store one word, `mprotect` back to read/exec. The vtable is in `.rodata` (section
flags `A`, 0x846508..0xa60e40), inside the executable's single R E `LOAD` segment, so the page is mapped read/exec and
holds only data (checked with `readelf -SW`/`-lW` on the stock player); no code runs from it while it is briefly writable. A data
store needs no cache maintenance. Finally `unsetenv("LD_PRELOAD")`. Status codes (log line `status=`): 1 installed,
-1 not the player, -2 symbols missing, -3 stock bytes differ, -4 mprotect failed.

## The replacement `process(self, in, out, midi, events)`

The live deck chain calls Wah with `in == out`: it is the second FX, after Crush. Verified from the decompile: every
`addBack` sorts the container with `AudioProcessFilterSorter`, whose rank table (ctor 0x499974, strings read from the
ELF) is transitioncomponent 0, reverse 1, loop 2, crush 3, **peq 4**, filter 5, trans 6, echo 7, roll 8, delay 9, reverb 10,
eq 11, and the names passed to the Crush/Peq constructors in `MixChannel::MixChannel` (0x493538) are exactly `crush` and
`peq`. Crush is always in the container and ranks below Peq, so Peq is never first and is always called `(buf, buf)`. With the flag file present:

1. Flag the shared buffer silent (`in[4] = 1`), call the original, restore the flag. The original then consumes all
   parameter events and stores the value at `self+0x80`, but skips its filter (decompile of 0x4f524c: the filter
   branch needs `in[4] == 0`; `copyFrom(x, x)` is a no-op). This keeps the stock parameter plumbing without
   touching `EventBuffer` internals. Verified against a stub that mimics the decompile; **not yet on a device**.
2. If the buffer was really silent, or `self[4]` (the stock bypass byte) is set: reset our state and return.
3. Read the value (`self+0x80`, a float -100..100, anything else counts as 0), map it, and run the DSP in place on
   the two float32 channel pointers (`buf+8`, `buf+12`, `buf+0x38` frames, `buf+0x34` channels >= 2, at most 4096
   frames, 4-byte aligned, not the same pointer).
4. `in != out`, flag file absent, or anything unexpected: call the original untouched (stock Wah).

State is kept per `PeqAudioProcess` instance (8 slots keyed by the object address, least recently used recycled). The
mix-document "tool" Wah shares the vtable, so a saved mix that uses Wah will also sound like Vocal while the flag is on.

No allocation, no locks, no stdio in the audio path. FPSCR flush-to-zero is set around the DSP and restored. The audio
path makes no syscall by default. The timing in the stats (`gettimeofday`, a real syscall on glibc 2.4 / kernel 2.6.19)
runs only if `revival-vocal-timing` existed at start: 2-3 calls on each of the first 64 blocks, then on 1 block in 64.
`EXTRA_CFLAGS=-DVFX_TIMING=0` compiles it out.

## DSP

Mid/side. The mid signal is split by two Linkwitz-Riley 4th-order crossovers (150 Hz and 4 kHz; each LR4 is two
identical Butterworth biquads) into low / mid / high; the low band goes through one all-pass biquad so the three bands
sum to a pure all-pass of the input, and the side signal goes through the same all-pass pair, so the stereo image
stays coherent. 11 biquads per frame, transposed direct form II, float32, run stage by stage over 128-frame
sub-blocks (independent chains interleaved for the in-order VFP). Per-band gains then do the work (see the table
above); they follow a per-sample one-pole smoothed amount (10 ms), and a second smoothed weight crossfades from the
untouched input to the processed signal. Processed samples above 0.97 pass a soft knee that tops out at 1.0 (the
all-pass can raise crest factors; the knee is high so that hot-mastered material at a near-neutral setting stays linear
below it). An input sample that is NaN, Inf or beyond 1e4 is replaced by 0 on entry, so a bad sample stays one sample. Denormals: FZ in FPSCR, plus a 1e-18 alternating offset into every chain so no
state ever decays into the subnormal range.

Why not "band-pass the mid and subtract it": a band-pass has phase, so `x - bandpass(x)` only cancels near one
frequency. Measured on this very signal path in an earlier version of the test: -2.4 dB on a voice, -7 dB at 1 kHz.
The three-band version gives -48 dB at 1 kHz.

### Verified on the PC (`test.sh`, native x86, float32 against the same code in double precision)

Output level vs input, dB RMS, settled (Wah value across the top):

```
signal \ Wah value            -100     -50     -25       0      25      50     100
centre voice 220Hz+harm      -16.8    -5.0    -2.2     0.0     0.5     1.0     1.8
side wideband noise            0.0     0.0     0.0     0.0    -2.5    -6.0   -93.1
centre bass 60Hz              -0.2    -0.1    -0.1     0.0    -2.4    -5.7   -29.1

centre tone (dB)       150   220   300   500  1000  2000  3000  4000  6000
cut -100              -6.0 -15.0 -24.6 -41.7 -47.9 -25.3 -12.7  -6.0  -1.4
cut -50               -2.5  -4.6  -5.5  -5.9  -6.0  -5.6  -4.2  -2.5  -0.7   (no notches at partial settings)
focus +100            -3.1   1.2   2.4   2.9   2.9   2.4   0.6  -3.1 -13.8
```

(The 220 Hz voice sits on the 150 Hz crossover, which is why its cut is only -17 dB; a voice centred higher is cut more.)
Also checked: idle blocks are bit-identical to the input and the state returns to idle 104 ms after release; output
is bit-identical for call sizes of 1, 500 and 20000 frames; amount steps 0 -> +100 -> -100 -> 0 produce no sample jump
beyond the signal's own slope; float32 differs from double by -77 .. -83 dB relative to the output; a NaN input
sample does not wedge the filter; full-scale noise with random settings never exceeds 1.0 after the soft knee
(2.0 without it); 10 s of silence after a burst produce no subnormal intermediate (instrumented build, FTZ/DAZ
off), and no output sample below 1e-30; a synthetic mix normalised to 0.9 stays at or below 1.0 at every setting (0.994 at the worst, cut -25/-50).
Also checked: a centre two-tone signal peaking at 0.8 / 0.95 is processed linearly at Wah +2 (nonlinear residue below
-100 dB), and at 0.989 with the worst of 12 phase offsets the residue is -49.5 dB re output RMS; NaN, +Inf, -Inf and 1e30 input samples give no non-finite output and no lasting disturbance;
the engage rule (`vfx_engage`) toggles once, not 176 times, for a value hovering around ±1.

### Verified with the ARM build under qemu-user and the device's ld.so / libc

`dlopen` of the library from a plain program works and the constructor refuses (status -1, "not pmplayer.real").
A stub executable named `pmplayer.real`, linked non-PIE with the firmware's real symbol addresses (process 0x4f524c,
destructors, 80-byte vtable at 0x877e48, `m_sampleRate`), is loaded with `LD_PRELOAD`: the slot is patched, calls go
through `vptr[0x14]` like the engine's, and with the flag file the ARM float results match x86 (focus +100 on 1 kHz
2.89 dB, cut -100 -47.88 dB, side noise at focus -91.7 dB in the stub's measurement). Flag absent: stock path runs
(stub's stand-in Wah halves the signal, measured 0.5000), log written. A silent buffer is left alone and its flag
restored; `in != out` goes to the stock path; a stub whose vtable is at another address is refused (status -3) and
stays stock. The periodic log thread fires after 5 s.

### Speed

On this PC (x86, one core): 47 ns per frame, 24 microseconds per 512-frame block. That says nothing about the ARM11.
The compiled ARM code was about 185 instructions per frame (about 160 of them VFP) for the DSP before the input sanitising, which adds roughly ten more (not re-counted); at 532 MHz that is
**my estimate** of 0.15 - 0.3 ms per block and deck, 1.5 - 2.5 % of the 11.6 ms block time. The real number is in the
first boot's log (`dsp avg=` in microseconds, `stock avg=` is the unmodified Wah call). Under qemu TCG the DSP
measured about 390 microseconds per block, which only shows that the timing code works.

## The log

`/mnt/public/.Pacemaker/revival-vocal.log`, rewritten (not appended) once at load, then every 5 s while the audio
callback is being called during the first five minutes, then **not periodically at all**: after five minutes it is
rewritten only when a milestone changes (first DSP block, first `nonalias`/`bad_buf`/`recycled`, instance count), checked
every 30 s. The partition is the one the PC mounts over USB, so steady write traffic is avoided. Example fields: `status=1`, `slot 0x00877e64: 004f524c -> ...`,
`dsp=ON|off(flag absent)`, `calls= dsp_blocks= bypass_blocks= stock_passthru= nonalias= silent= bad_buf= inst= recycled=`,
last and maximum |Wah value| seen, frames per call (expect 512), and sampled timings in microseconds with the last 16.
The log is written from a separate thread (64 KB stack, created in the constructor), never from the audio thread.

## First boot checklist

1. Image with library + wrapper snippet + lang file, flag file **absent**, `revival-vocal-timing` **present** (this boot
   is the measurement). Boot, play a track with FX on. Expect the log to show `status=1`, `calls` growing, `frames=512`,
   `stock_passthru` = calls, `nonalias=0` (if it grows, the Wah is not second in the chain after all and the DSP will never
   run: stock behaviour, report the log) and `stock avg` as the baseline. Delete `revival-vocal-timing` after this boot.
   The sound must be exactly stock.
2. Create `revival-vocal`, reboot. Play a track with a centred vocal, select the Wah slot ("Vocal") on a pad
   direction. Check: the pad value reaches us (`param last=` follows the pad), `dsp_blocks` grows, no dropouts, the
   log's `dsp avg`. If the value never changes while the pad moves, the silent-flag event pump does not work as
   decompiled: delete the flag file and report the log.
3. Any trouble: create `revival-no-preload` (or delete the flag file) and reboot.

## Known limits and risks

- The silent-flag event pump, the `self+0x80` value read and the `in == out` assumption come from the decompile; they
  match the stub but are untested on the real player. The `AudioProcessBuffer` offsets (+4, +8, +0x34, +0x38) are from
  the constructor decompile at 0x4ae280 and the `copyFrom` decompile at 0x4ae168.
- A pthread is created in the constructor (log writer) before `main`. Not tried on the device's libpthread (qemu ran
  glibc 2.4's, but the device kernel is 2.6.19.2).
- If a C++ exception ever propagates out of the stock `process()`, it unwinds through the hook (the library has
  `.ARM.exidx`; shown under qemu with a forced unwind, and the same test without unwind tables stops at the hook frame),
  but the silent flag of the shared buffer is then not restored for that block. Nothing in the decompile of `process()`
  throws, so this is a theoretical path.
- Crash in the hook = player abort. All the pointer and size checks above exist for that reason.
- Firmware specific: other builds refuse (status -3) and stay stock.
- Float32 filter noise is about -77 dB relative to the processed signal in the deepest cut; fine for DJ use, not mastering.
