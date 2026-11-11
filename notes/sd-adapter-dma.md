# SD adapter: audio stutter while loading, and an opt-in DMA path (nothing flashed or run on a device)

Tags: [V] verified in this session (disassembly of the shipped modules, decompilation of `pwrApplication`, a run
of the shell logic against stubs), [I] inferred from verified facts, [H] hypothesis / unverified.
Firmware 180096069 rootfs (`work/rootfs-180096069`, kernel 2.6.19.2, `ide-core.ko`, `mxc_ide.ko`, `ide-disk.ko`
disassembled with `objdump -dr -marm`), custom 180096072 on top. Read first: `sd-adapter-lba.md`, FINDINGS 5c.

## Short answer

- [V] The stutter hypothesis is consistent with the code, but not yet measured. The adapter is in PIO because
  `id->capability` (IDENTIFY word 49, high byte 0x6c) has the DMA bit clear. In PIO every byte is moved by the ARM
  from the interrupt handler, with interrupts masked unless `unmaskirq` is 1.
- [V] The decision is in two modules, not one: `ide-core.ko` refuses `hdparm -d 1` (EPERM), and `mxc_ide.ko` never
  enables DMA by itself unless the drive reports the DMA bit *and* a selected UDMA/MWDMA mode. `ide-disk.ko` has no
  DMA logic.
- [V] The kernel does not pick a PIO mode from the garbage words and does not fall back to PIO 0: the host is left at
  the PIO 4 timing programmed at module load. So there is nothing to patch for PIO mode selection.
- [V] The boot-time `hdparm -d 1 / -X udma5 / -S0` are not in any script. They are string constants in
  `pwrApplication` (`HddInterface::loadHddModules`), run before the first mount. `hddmount.sh` is only run by the
  factory tool `pmptest`.
- Delivered: `scripts/patch-ide-dma.py` (ide-core.ko: 2 instructions; pwrApplication: 1 byte), `scripts/device/
  pmplayer-wrapper.sh` (diagnostic, PIO/DMA timing, flag-file DMA with verified fall back), `scripts/device/
  hddmount.sh` (the stock script with DMA moved behind the mounts and made conditional).

## 1. Who decides DMA, and what the kernel does with this IDENTIFY block

Adapter IDENTIFY words (from FINDINGS 5c / the diag): 49 = 0x6c61, 62 = 0x797a, 63 = 0x0007, 64-67 = 0x2020,
88 = 0x003f, 60-61 = 125,042,688 sectors. Word 49 is read as `id->capability` = byte 99 = 0x6c: bit 0 (DMA) clear,
bit 1 (LBA) clear, bit 3 (IORDY) set.

[V] Every read of byte 99 in the three modules (`ldrb rX,[rY,#99]`):

| Where | What it does with `capability` |
|---|---|
| `ide-core` `set_using_dma` (.text+0xdc0, the HDIO_SET_DMA / `using_dma` setting) | bit 0 clear -> return -EPERM before calling anything. This is why `hdparm -d 1` fails. |
| `ide-core` `ide_do_request`, PM-resume step (.text+0x35f4) | bit 0 set -> `hwif->ide_dma_check(drive)` after a resume. |
| `ide-core` `ide_get_best_pio_mode` (.text+0x6d98) | bit 3 (IORDY). Not called by `mxc_ide` (below). |
| `mxc_ide` `mxc_ide_dma_check` (.text+0xc58, the auto path) | see below |
| `ide-disk` | only the LBA bit (see `sd-adapter-lba.md`). |

[V] `mxc_ide_dma_check` (called for both drives from `init_module` after `ide_register_hw`, from `mxc_ata_init`
after every resume, and by `set_using_dma`) turns DMA on only if all of this holds, else
`ide_dma_off_quietly` (clears `drive->using_dma`, returns 0):

1. `capability & 1`;
2. `field_valid & 4` (word 53 bit 2) and `dma_ultra & 0x7f00` (word 88, the *selected* UDMA mode bits), or
   `field_valid & 2` and `(dma_mword & 0x404) == 0x404` (word 63: MWDMA2 supported and selected) or the same for
   word 62, or `__ide_dma_good_drive` (model whitelist).

For this drive it fails at step 1, and it would also fail step 2: word 88 = 0x003f has no selected bit, word 63 =
0x0007 has no selected bit, the model is not on the whitelist. So even an adapter with a correct word 49 would stay
in PIO under this driver until something sends SET FEATURES; the stock HDD got there, [H], because a drive that powers
up with a UDMA mode already selected passes it.

[V] `mxc_ide_dma_on` (.text+0xb4c, hwif->ide_dma_on): rejects drives on `__ide_dma_bad_drive` (model blacklist; "iFlash" is not in `ide-core`'s strings, so this adapter is not on it [V]),
then `printk("%s: enabling UDMA3 mode")`, `mxc_ide_set_speed(drive, 0x43)` (host timing) and
`ide_config_drive_speed(drive, 0x43)` (SET FEATURES to the drive), then sets `drive->using_dma`. The mode is
hard-coded UDMA3 and the return value of `ide_config_drive_speed` is ignored. It does not look at the ID words.
So once `set_using_dma` lets the request through, `hdparm -d 1` yields UDMA3 on host and drive, whatever the ID says.

[V] `hdparm -X <mode>` goes through `ide_cmd_ioctl`: `set_transfer()` (needs `dma_ultra != 0`, word 88 = 0x3f is
enough), then for modes above UDMA2 `ide_ata66_check()` (word 93 bits 13/14 must be set and `hwif->udma_four`,
which `mxc_ide_dma_init` forces to 1). If the check fails the kernel prints "Speed warnings UDMA 3/4/5 is not
functional" and the ioctl returns 0 without sending anything. So UDMA0-2 are never filtered, UDMA3-5 depend on
word 93 [unknown, text overflow]. That is why the first level to try is udma2, and why the boot-time `-X udma5` may
be a silent no-op already (the diag's kernel log will show the warning if so). If it does go through, it sends SET
FEATURES to the drive and `ide_set_xfer_rate` -> `mxc_ide_set_speed` reprograms the host to match.

## 2. PIO: which mode, which timing

- [V] `mxc_ide` never calls `ide_get_best_pio_mode`. At module load (`init_module`) it calls
  `set_ata_bus_timing(4, PIO)`: PIO 4 on the host. `probe_hwif` then calls `hwif->tuneproc(drive, 255)`
  (auto-tune, 0xff) which is `mxc_ide_tune` -> `set_ata_bus_timing(255, PIO)`; mode 255 > 4 returns -EINVAL and
  changes nothing except rewriting the common time_off/time_on bytes to 3. The same call is made after a PM resume
  (.text+0x35b4). So the garbage in words 48-53/62-68 never reaches the timing code and there is no PIO 0 fallback.
- [V] The drive itself is never sent a PIO mode (no SET FEATURES transfer mode for PIO anywhere in `mxc_ide`).
  It stays in its power-up PIO mode.
- [V] PIO 4 timing as the driver computes it (`pio_specs` row 4 = 120/25/70/70/0/10/10/50 ns, T = 1e9 / `ata_clk` rate
  rounded down): time_1 = (25+T)/T, time_2 = (70+T)/T, time_9 = (10+T)/T clocks. With T = 15 ns [H: ATA clock =
  IPG clock 66.5 MHz, not read from the device] that is 2 + 5 + 1 = 8 clocks = 120 ns per 16-bit cycle = 16.7 MB/s
  on the wire. For comparison the driver would give PIO 0 27 clocks = 405 ns (it ignores t0 = 600).
- [V] UDMA2/UDMA3 rows (`udma_specs`, tcyc 55 / 39 ns) lead to 4 / 3 clocks per 16-bit word at the same assumed T (60 ns / 45 ns, 33 / 44 MB/s on the wire).
- [H] `mxc_ata_init` (generic resume) turns the ATA clock off and on and only programs UDMA5 timing afterwards, not
  PIO. Whether the PIO registers survive the clock gating is not known. If they do not, PIO after a resume would run
  on the reset values. The diag's `hdparm -t` per start would show a slowdown after wake.
- [H] `ATA_CONTROL` is written with 0x40 (only ATA_RST_B) at init. If bit 0 is IORDY_EN (i.MX31 manual, from memory,
  not checked here), PIO runs without IORDY flow control, which is fine as long as the adapter is fast enough.
- Conclusion: no PIO patch. It would change nothing: the host is already at the best PIO mode.

## 3. What is expected: throughput and CPU for a 10 MB load

None of this was measured on the device [I], the next diag will do it. Assumptions: 66.5 MHz ATA clock, ARM11 at
399 MHz, microSD behind the adapter good for about 15-25 MB/s sequential.

| Path | Bus ceiling | What limits it | CPU for 10 MB |
|---|---|---|---|
| PIO 0 (405 ns/cycle as the driver times it; 600 ns by spec) | 4.9 MB/s (3.3 spec) | CPU+bus | 2-3 s, about 100% of the core |
| PIO 4 (what the driver programs) | 16.7 MB/s | each `insw` stalls the ARM on the ATA bus (120 ns bus cycle plus the AHB path to the ATA block); realistic 5-8 MB/s | about 1.3-2 s, about 100% |
| UDMA2 (33 MB/s) / UDMA3 (44 MB/s) | 33 / 44 MB/s | the SD card; transfers run by the SDMA engine | tens of ms: one interrupt and one SDMA setup per request (up to 64 KB: `blk_queue_max_segment_size(0xfc00)`, 42 segments), so about 160-320 requests, about 1-3% |

Why that stutters, [H]: during a PIO load the core is fully busy for 1.3-2 s per 10 MB, and with `unmaskirq 0`
(the default) each interrupt handler moves `multcount` sectors with interrupts masked. At 6 MB/s that is 85 us per
sector, so `multcount 16` is a 1.4 ms blackout and `multcount 128` an 11 ms one, which an audio engine with small
buffers does not survive. `multcount`, `unmaskirq` and `io_32bit` are in the diag (`/proc/ide/hda/settings`); the
wrapper also records `/proc/stat` and the IDE interrupt count around each `hdparm -t`, so CPU share and interrupts
per MB are measured rather than guessed. The cheap test for the second cause is the `revival-unmask` flag (below),
which is the classic cure for PIO stutter and needs no kernel change.

## 4. The patch (`scripts/patch-ide-dma.py`)

All checks as in `patch-ide-disk.py`: stock bytes at the site and of the instructions around it, no relocation
within the site, same size and section table, `__versions`/`.modinfo`/`.rel.text`/`.symtab` byte-identical, output
re-read, refuses on an already patched file. Output `work/build/ide-core-dma.ko` (110,844 bytes, 6 bytes differ).

| Where | Before | After | Effect |
|---|---|---|---|
| `ide-core.ko` .text+0xde8 (`set_using_dma`) | `0a000011` `beq 0xe34` | `e1a00000` nop | `hdparm -d 1` / `-d 0` stop returning EPERM for a drive without the DMA bit. `-d 1` then runs `mxc_ide_dma_check` (off, returns 0) and `mxc_ide_dma_on` (UDMA3). |
| `ide-core.ko` .text+0x3600 (`ide_do_request`, PM resume) | `0a000005` `beq 0x361c` | `e1a00000` nop | After an HDD standby / wake the driver now always calls `ide_dma_check`. For this ID it switches DMA off, so the drive wakes in PIO instead of with `using_dma = 1` and an un-negotiated drive. Drives that report the bit take the same path as before. |

Not changed: `mxc_ide.ko`. Its auto path still needs the DMA bit, so DMA is never on without a userspace request.

Second output, `work/build/pwrApplication-ring-nodma` (input default `work/build/pwrApplication-ring`, the
recoloured binary that is installed; or pass the stock one): one byte, `hdparm -d 1 /dev/hda` ->
`hdparm -d 0 /dev/hda` in the string table at file 0x1b2d48. Checks: the segment layout, the bytes around it (the
`modprobe mxc_ide` string before, `hdparm -X udma5` after), exactly one literal-pool reference
(0x61dac, `loadHddModules`). Without it, the patched ide-core would let that boot-time call succeed and put the
adapter in UDMA3 before the first mount, with no way to condition it on the flag file (the FAT volume is not
mounted yet) and with a mount failure leading, after five tries, to the "Restore hard drive?" prompt. With it, boot
is PIO exactly as today; `-X udma5` (kept) is as harmless as it is now.

Install (nothing done here):

    wsl -d Debian -u root -- scripts/build-rootfs.sh STOCK.pfw OUT.cramfs <next FWVERSION> \
      /lib/modules/2.6.19.2/kernel/drivers/ide/ide-disk.ko=work/build/ide-disk-lba.ko \
      /lib/modules/2.6.19.2/kernel/drivers/ide/ide-core.ko=work/build/ide-core-dma.ko \
      /usr/local/bin/pwrApplication=work/build/pwrApplication-ring-nodma \
      /usr/local/bin/hddmount.sh=scripts/device/hddmount.sh

(plus whatever else the current 180096072 build carries). The image side is `build-pfw.sh --diag`; its heredoc
wrapper is replaced by `scripts/device/pmplayer-wrapper.sh` (not wired into `build-pfw.sh` here, to avoid a
conflict with the other branches that edit the same wrapper; install it as `/bin/pmplayer`, mode 0755, with the real
player kept as `pmplayer.real` exactly as `--diag` does, and keep the final `exec`).

Versions: the stock module is rebuilt against `__versions` unchanged, vermagic `2.6.19.2 preempt mod_unload ARMv6`
untouched; no kernel image change.

## 5. The wrapper and the flags

`.Pacemaker/` on the FAT volume, create or delete the files over USB. Nothing is needed for a normal boot.

| File | Effect |
|---|---|
| `revival-dma` | After the mounts, try DMA. First word may be `udma0`..`udma5` / `mdma0`..`mdma2`; default `udma2` (33 MB/s, never filtered by the 80-wire check). Sequence: md5 of 8 MiB of `/mnt/private/active_sw_file` (PIO, page cache dropped), `hdparm -d 1` (kernel: UDMA3), `hdparm -X <level>` (kernel: SET FEATURES + host timing), md5 again, kernel log. Any mismatch, new IDE error line or `using_dma 0` -> `hdparm -d 0`, and the diag says so. |
| `revival-unmask` | `hdparm -u1` at the start of the storage section. |
| `revival-nobench` | Skip the `hdparm` timings. |

Per boot, the first player start writes `.Pacemaker/revival-diag.txt` fresh with: partitions, `/proc/ide/hda/{identify,
model,capacity,geometry,media,driver,settings}`, `hdparm`, `hdparm -i`, `hdparm -I`, `/proc/interrupts`, whether
word 49 bit 8 is set, the settings of interest (`using_dma`, `unmaskirq`, `multcount`, `io_32bit`, `current_speed`,
`pio_mode`), the IDE lines of `dmesg`, then `hdparm -tT` in PIO with CPU split (user / system / irq / iowait /
idle, from `/proc/stat`) and the IDE interrupt count before and after, the DMA attempt, and `hdparm -t` again in
DMA with the same CPU and interrupt figures. Later starts in the same boot (marker in `/tmp`) append a short
record (state, kernel log) and retry DMA if it was lost; the file is truncated beyond 400 KB. A drive that reports
DMA itself still gets the stock `hdparm -d 1` / `-X udma5`, there, after the mounts.

[V] The shell logic was run under dash against stubbed `hdparm`/`dmesg`/`md5sum` and fake `/proc` files for: no
flag, flag + clean DMA, flag + md5 mismatch, flag + kernel error line, kernel refusal, restart after a lost DMA,
`unmask`, `nobench`, and a drive that reports DMA. It uses only applets listed in the rootfs's busybox 1.1.3
(`awk`, `grep -c -E`, `head`, `wc`, `md5sum`, `dd`, `sync`, `dmesg`, `date`) plus the `read`, `case` and `${v#?}` shell built-ins; no `printf`, no `tr`, no `sed`.

Costs and risks: the first start of a boot takes about 6 s longer (PIO `-tT`), 9-15 s with the DMA flag (two
8 MiB reads in PIO and DMA plus `-t`). `pwrApplication` forks the player and does not wait for it in a way I could
see (`FunctionalStateMachine::createChild`), but a start-up watchdog elsewhere is not excluded; `revival-nobench`
is the escape. The player image itself is on the drive, so a DMA fault during the test could hit the loop-mounted
image; that is what the read-back check is for. Known limit: DMA does not survive an HDD standby / wake (see the
patch table); the diag shows `using_dma` at each start. Making resume restore it would need a code cave in
`mxc_ide_dma_check` and was left out.

## 6. Open points for the next boot

1. `/proc/ide/hda/settings`: `multcount`, `unmaskirq`, `io_32bit` (not captured so far).
2. Word 53 and 93 of IDENTIFY, `hdparm -I` output, and whether the kernel log has "Speed warnings UDMA 3/4/5".
3. The PIO `hdparm -tT` figures and the CPU split. If PIO 4 already gives 10+ MB/s with low CPU, DMA is not the
   cause and `unmaskirq` / `multcount` are next.
4. With `revival-dma`: the verdict line, the DMA figures, and the "enabling UDMA3 mode" message.
