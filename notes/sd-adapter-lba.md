# SD adapter: forcing LBA28 in ide-disk.ko (no flashing done)

Tags: [V] verified in this session (disassembly / run), [I] inferred, [H] hypothesis / unverified.

## Where the decision lives

- [V] The device loads `ide-disk` as a module: `formathdd.sh`, `hddmount.sh`, `gadget.sh` and `formatswap.sh` run
  `modprobe ide-disk` when the kernel patch level is not 28 (2.6.19.2 -> `ide-disk`), and `modules.dep` lists
  `kernel/drivers/ide/ide-disk.ko: ide-core.ko`.
- [V] The kernel image `work/build/vmlinux-180096069.bin` (3,085,936 bytes, decompressed; contains one
  "Linux version" string) contains none of `ide-disk`, `idedisk`, `Host Protected Area`, `ide_register_subdriver`,
  `hd_driveid`, `mxc_ide`. IDE core is `ide-core.ko`, the host is `mxc_ide.ko`. So nothing about capacity/LBA is
  built into the kernel; only a module needs patching.
- [V] In `ide-core.ko` the only loads of `id->capability` (`ldrb [rX,#99]`) test bit 0 (DMA: `set_using_dma`,
  `ide_do_request`) and bit 3 (IORDY: `ide_get_best_pio_mode`). Nothing there tests bit 1 (LBA, 0x2 of the high
  byte of word 49). So the LBA decision is entirely in `ide-disk.ko`, and DMA is untouched by any change there.
- [V] In `ide-disk.ko` the byte `id->capability` is loaded exactly once (`ldrb r3,[r4,#99]` at .text+0x1d2c).

## The code (ide-disk.ko, ARM, .text at file offset 0x34)

`init_idedisk_capacity()` and `lba_capacity_is_ok()` are inlined into `ide_disk_probe` (.text 0x1528..0x1dec);
`idedisk_check_hpa` (0x628) and `set_lba_addressing` (0x30) are separate functions.

- [V] LBA48 test (0x1960-0x1998): word 83 bit 10 (`[id,#166]`), word 86 bit 10 (`[id,#172]`), words 100-103
  (`[id,#200/#204]`) non-zero. Adapter: word 83 = 0x5004 (bit 10 clear) -> not taken.
- [V] The not-LBA48 path jumps to 0x1d2c:

      1d2c  ldrb r3, [r4, #99]    ; id->capability (word 49 high byte)
      1d30  tst  r3, #2           ; LBA bit
      1d34  beq  1a88             ; clear -> CHS capacity (cyl*head*sect, select.lba stays 0)
      1d38  b    19b8             ; set   -> inlined lba_capacity_is_ok(), then LBA28

  Word 49 = 0x6c61 -> high byte 0x6c -> bit 1 clear -> the `beq` is taken -> `1a88` CHS: 16383*16*63 =
  16,514,064 sectors. This matches the observed dmesg.
- [V] `lba_capacity_is_ok` (0x19b8-0x1a58) for the adapter's numbers: cyls (`[id,#2]`) = 16383 = 0x3fff ->
  `19e8`; sectors (`[id,#12]`) = 63 ok; heads (`[id,#6]`) = 16 (15 or 16 allowed); then
  `lba_capacity (id+120, words 60-61) >= heads * 0xfbfc1 (=16383*63)`: 125,042,688 >= 16,514,064 -> `bcs 1a5c`.
  `1a5c` sets `select.lba` (`[drive,#68] |= 0x40`) and stores `capacity64 = lba_capacity` (`strd` at 0x1a74).
  Words 60-61 = 0x0000 / 0x0774 -> little-endian dword at id+120 = 0x07740000 = 125,042,688.
- [V] LBA28 addressing: `set_lba_addressing` (0x30) leaves `drive->addressing = 0` unless word 83 bit 10, word 86
  bit 10 and words 100-103 are all set, which they are not. `ide_do_rw_disk` chooses LBA vs CHS from
  `drive->select.b.lba` (byte 68 bit 6, e.g. 0x103c-0x104c), which `1a5c` sets.
- [V] DMA is decided by `id->capability & 1` in ide-core (above), unchanged: still PIO.

## The patch (one instruction)

| Where | Before | After |
|---|---|---|
| `.text+0x1d34` = file offset `0x1d68` | `53 ff ff 0a`  `beq 0x1a88` | `00 00 a0 e1`  `mov r0, r0` (nop) |

With the branch removed, control always reaches `b 19b8`, i.e. the stock `lba_capacity_is_ok` check, then LBA28.
`1d2c`/`1d30` still execute but their results are dead.

- [V] Produced by `scripts/patch-ide-disk.py` -> `work/build/ide-disk-lba.ko` (19,676 bytes, same as stock).
  `cmp -l` shows exactly 4 bytes differ (file offsets 7529-7532). Disassembly after:

      1d2c: ldrb r3, [r4, #99]
      1d30: tst  r3, #2
      1d34: nop                 (mov r0, r0)
      1d38: b    19b8

- [V] Relocations: `.rel.text` has no entry in 0x1d31..0x1d37; the nearest are `R_ARM_CALL` at 0x1d10/0x1d20
  and literal-pool `R_ARM_ABS32` from 0x1d44. The script checks this and refuses otherwise. The patched word is a
  plain PC-relative conditional branch to a local label, not a relocated instruction or literal.
- [V] The script verifies before writing: the 4 stock bytes, the surrounding instructions
  (`ldrb`, `tst`, `b 19b8`), no overlapping relocation; after patching: same length, identical section header table,
  identical `__versions`, `.modinfo`, `.rel.text`, `.symtab`, `.strtab`, `.data`, `.rodata.str1.4`,
  `.gnu.linkonce.this_module`, only the 4 bytes differ, and it re-reads the written file. Running it on the
  already-patched file refuses (idempotency guard). `readelf -h` parses the output.

## Why it is safe for drives that report LBA correctly

- [V] If the LBA bit is set, the stock `beq` was not taken and execution went to `b 19b8`. The patched code does
  the same, so behaviour is identical for such drives (the stock 120 GB Samsung HS122JB included).
- [I] For a drive with the LBA bit clear, the new path runs `lba_capacity_is_ok`. A true CHS-only drive reports
  words 60-61 = 0, which fails the check (cyls != 16383: "lba within 10% of chs" fails, the word-swapped 0 also
  fails -> `1a88` CHS, as before). Only drives with non-zero words 60-61 consistent with their CHS geometry change,
  and those are effectively LBA drives. Corner case: IDENTIFY cyls = 0 with LBA bit clear is accepted as LBA by the
  stock check (`beq 1a5c`), but the stock CHS capacity would also be 0 there.

## Expected result on the device

- [I] dmesg should show `hda: 125042688 sectors (64022 MB)` and still `using_dma 0`.
- [I] The geometry reported to userspace (HDIO_GETGEO) becomes cylinders = min(capacity / (16*63), 65535) =
  65535, 16 heads, 63 sectors (CHS covers only 33.8 GB). This is the standard 2.6.19 `idedisk_setup` bios-geometry
  logic and is what the 120 GB Samsung produced. The device's `fdisk_hdd` input (delete 1-4, new primary 2 =
  +500M, new primary 1 = defaults, type c on 1) takes its sizes from BLKGETSIZE or from this geometry; which one
  busybox fdisk uses for "default last" is [H]. If the FAT32 partition ends up near 33 GB instead of 63 GB,
  partition from a PC (MBR, p2 500 MB ext3, p1 FAT32 for the rest).
- [H] HPA: `idedisk_check_hpa` (READ NATIVE MAX / SET MAX) runs only if words 82 and 85 bit 10 advertise HPA. The
  adapter's words 82/85 are not in the notes. The CHS path never called it; the LBA path may. Low risk.

## Do I need to re-partition?

- [V/I] Yes. The existing partitions end at ~8.45 GB and FAT32 is 7.9 GB; changing the reported capacity does not
  move or grow them. Re-run `formathdd.sh` on the device (erases the card) or partition and format from a PC with the
  same layout as `fdisk_hdd` (p2 500 MB ext3 first, p1 FAT32 label Pacemaker for the rest). Until then the old
  layout, being inside the larger capacity, remains readable.

## Module loader checks (2.6.19.2)

- [V] `.modinfo`: `vermagic=2.6.19.2 preempt mod_unload ARMv6 `, `depends=ide-core`, `license=GPL`. Untouched.
- [V] `__versions` (modversions CRC table, 0xe40 bytes) is byte-identical; CRCs describe imported symbols, not
  this module's code.
- [I] 2.6.19 has no module signatures and no checksum over the .ko; the gates are vermagic, modversions CRCs,
  symbol resolution and ELF/relocation processing. The patched file has the same size, section table and
  relocations, and `modules.dep` is unchanged.

## Installing (not done)

`scripts/build-rootfs.sh STOCK.pfw OUT.cramfs FWVERSION /lib/modules/2.6.19.2/kernel/drivers/ide/ide-disk.ko=work/build/ide-disk-lba.ko`
(per that script's usage line; needs root in WSL), then a normal rootfs update. Nothing has been flashed or installed.
