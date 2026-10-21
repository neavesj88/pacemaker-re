# Replacing the Pacemaker hard drive: what a blank drive must contain

Static analysis only, nothing run on a device. Tags: **[V]** verified in a shipped binary or script
(decompiled or read), **[I]** inferred from verified facts, **[H]** hypothesis, untested.

Sources: `pwrApplication` from firmware 180096069 (the version on the owner's device, system.xml
confirms `SWVERSION`/`FWVERSION` 180096069, bootloader 12838), decompiled with
`scripts/ghidra-decompile.sh` into Ghidra project `work/ghidra/ps180-pwrApplication` (image base
0x8000, so file offset + 0x8000 = address). The 14142 `pwrApplication` carries the same strings for
every path name quoted here (checked with `scripts/strs.py` / `wstrs.py`) but was not decompiled.
Extra reading: `usr/local/bin/{formathdd.sh,fdisk_hdd,hddmount.sh,gadget.sh}` (identical layout in
both rootfs), `usr/local/share/fonts/english.lang`, Mac Editor `DeviceCommunication::updateFirmware*`.
The device kernel is Linux 2.6.19.2 (decompressed from `boot/zImage`), `mke2fs` in the rootfs is
e2fsprogs 1.34 (25-Jul-2003), `fdisk` is util-linux.

## Short answers

1. The system image is **`/mnt/private/active_sw_file`**, i.e. a file in the root of the ext3
   partition hda2. It is a verbatim copy of a `.pfw` (ext3 image plus the 512-byte trailer).
2. The device scans **`/mnt/public/.Pacemaker/`** (non-recursive, any file name) on the FAT32
   volume for a valid `.pfw`, moves the newest valid one to `/mnt/private/pre_active_sw_file`,
   creates the flag `/mnt/private/upgrade_in_progress`, flashes if versions differ, reboots, and
   on the next boot copies it to `active_sw_file`.
3. `formathdd.sh` runs only when the user confirms a "Restore hard drive?" dialog (reached
   automatically when hda2 or hda1 cannot be mounted five times) or from the key-combo debug menu or
   production-test mode. Never silently.
4. A blank drive can be prepared on a PC with two partitions (FAT32 LBA hda1 + ext3 hda2) and either
   a `.pfw` in `hda1:/.Pacemaker/` (device installs it) or `active_sw_file` on hda2 (device boots
   straight away). The device can also provision a fully blank drive itself, then needs the `.pfw`
   over USB mass storage. Details below.

## 1. What lives on hda2 (ext3, mounted rw at /mnt/private)

Mount command (HddInterface::mountPrivateExt3 @0x69f80) [V]:
`mount -t ext3 -o noatime,nodiratime /dev/hda2 /mnt/private`, retried up to 5 times.

| Path on hda2 | Role | Evidence |
|---|---|---|
| `active_sw_file` | The running system image. `System::mountActiveDmgFile` (@0x72b10) runs `mount -r -t ext3 -o noatime,nodiratime,loop /mnt/private/active_sw_file /mnt/vf` (the string table has `"mount -r -t ext3 -o noatime,nodiratime,loop "` + path + `" "` + `"/mnt/vf"`). `/pmds` is a symlink to `/mnt/vf`, so `/pmds/bin/pmplayer` (launched by `FunctionalStateMachine::createChild` @0x6b798 with `execv`) comes from this file. The only existence test is `File::exists` (`isActiveDmgFileOk` @0x72920). | [V] |
| `pre_active_sw_file` | Staging copy of the `.pfw` during an update. Stays on disk afterwards. | [V] |
| `upgrade_in_progress` | Flag file created by `prepareSwUpgrade`, deleted by `finalizeSwUpgrade`. `finalizeSwUpgrade` opens it with `open64(path, 0x1002, 0666)` (no O_CREAT), so it only counts if it already exists. | [V] |
| `swapfile` | 132207 KiB (about 129 MiB). `HddInterface::isPrivateExt3Ok` (@0x68cf8) is literally "does /mnt/private/swapfile exist". If missing, `repairPrivateExt3` (@0x69934) runs `dd if=/dev/zero of=/mnt/private/swapfile bs=1024 count=132207; chmod 600; mkswap`. Self-healing, not required up front. | [V] |
| `language_setting.dat` | One byte, an index into the language table. `is::loadAndSetLanguage` (@0x8f5a8) opens it, reads 1 byte, and falls back to English (index 0) if it is missing or out of range. pmplayer also references the path. Optional. | [V] |
| `vfat_mounted` | Flag file set/cleared by `HddInterface::setVfatMountFlag` (touch / rm -f). Deleted by the Clean FS debug action. | [V] |
| `pmic_adc_log` | Log file deleted by Clean FS. | [V] |
| `activatedebugmount` | Developer flag: if present, `mountDmg` mounts the image read-write and without `-r`. | [V] |
| `lost+found` | From mkfs. | [I] |

Space: `active_sw_file` 50,000 KiB + `pre_active_sw_file` 50,000 KiB + swapfile 132,207 KiB =
about 227 MiB, which fits the 500 MiB the factory script gives hda2 [I from sizes].

On the FAT32 side (hda1, mounted `-t vfat -o noatime,nodiratime` at /mnt/public) the device keeps
`.Pacemaker/{music.db,settings.dat,system.xml,syncfile,Music/,tmp/,dump/}` plus `.Pacemaker/activatedebugmode`
(flag). `/pmdb` and `/pmdb_tracks` are symlinks to `/mnt/public/.Pacemaker` and
`/mnt/public/.Pacemaker/Music` [V, link targets read from the extracted cramfs].

`HddInterface::isPublicVfatOk` (@0x69760) = `.Pacemaker`, `.Pacemaker/Music` and `.Pacemaker/tmp` all
exist; otherwise `repairPublicVfat` (@0x69840) creates them. So an empty FAT32 is self-healing [V].
`music.db` is created by pmplayer if absent (FINDINGS.md section 3).

## 2. Firmware update flow (.pfw on FAT32 to active_sw_file)

Everything is in `FunctionalStateMachine::initState` (@0x6d398, runs at every boot and every time the
state machine returns to INIT), `SWUpgrade::*` and `PfwFile::*`.

1. **Where it looks.** `SWUpgrade::prepareSwUpgrade` (@0x714bc) calls `getLatestPfwFileOnHdd(dir =
   "/mnt/public/.Pacemaker")` (@0x71154), which does
   `File(dir).findChildFiles(results, 2 /*files*/, false /*not recursive*/, "*")`. **Any file name
   works** (there is no `.pfw` literal anywhere in the binary, only the log strings "PFW File Found").
   Hidden files are included (JUCE 1.46 `findChildFiles` flag semantics from memory; same call is used by
   `deleteAllPfwFiles`). [V for the call, I for the semantics]
2. **What counts as valid** (`PfwFile::analyze` @0x70548) [V]:
   - trailer start = file length - 512;
   - `isBlockCompatible` (@0x6faa4): the first block at trailer offset 0 (2 hex length digits then
     value) parsed as an integer must equal 1 (the `ver` field);
   - `isMd5SumOk` (@0x700fc): reads the 34-byte block at trailer offset 0x1DE (length digits + 32
     hex), computes MD5 over the whole file minus the last 34 bytes (`MD5(stream, size - 0x22)`) and
     string-compares to the stored value;
   - then reads `tag`, `swver` (offset 0x84, parsed as 64-bit), `fwver`, `bootloaderver`, `uid`.
   Anything that fails is simply ignored (it stays in the folder). I re-implemented the first two
   checks in Python on all seven files in `vendor/firmware/`: all pass (`ver` 1, md5 equal), which
   agrees with `scripts/pfw-trailer.py`.
3. **Which one wins.** Among valid files the highest 64-bit `swver` is kept and every other valid
   `.pfw` in the folder is deleted (`System::deleteFile`). [V]
4. **Gate.** `prepareSwUpgrade` first does `moveFileTo("/mnt/private/pre_active_sw_file")`
   (failure: both files deleted, returns -1, on-screen "FW Error / Invalid file", language ids 267/268).
   Then it requires the 64-bit `fwver` > 12837 **and** `bootloaderver` > 12837 (0x3225). All seven
   shipped images pass (bootloader 12838). Note it is *not* a "newer than installed" test, so any of
   the seven images is accepted, including older ones. On success it logs "Start sw upgrade",
   `File::create("/mnt/private/upgrade_in_progress")`, `sync()`, returns 0. [V]
5. **Update state** (`fwUpgradeState` @0x6c648, screen "Please wait! / Firmware upgrade / in
   progress"): `startFwUpgrade` (@0x70630) loop-mounts `pre_active_sw_file` at `/mnt/tmp`
   (`mount -r -t ext3 ... loop`), re-analyses it, then
   - if the image's bootloader number differs from the 32-bit value read from `/dev/mtd/1` it
     writes `/mnt/tmp/boot/redboot.bin` to `/dev/mtd/0`;
   - if its `fwver` string differs from `FWVERSION` in the running `/etc/system.xml` it writes the
     status block (`A10...`) to `/dev/mtd/1`, `boot/zImage` to `/dev/mtd/2` and
     `boot/cramfs.little` to `/dev/mtd/3` or `/dev/mtd/4` (depending on whether `/dev/mtd/8` exists);
   - if the version is **equal** nothing is flashed, except that a status block starting `A00`
     (set after a DFU restore) is rewritten to `A10`.
   It then waits until at least 10 s have passed and calls `IOApi::wdiReboot`. [V]
6. **Next boot** (`finalizeSwUpgrade` @0x70c74): status block `A00` in `/dev/mtd/1` returns 4 ("DFU has
   been used for SW upgrade"); otherwise, if `upgrade_in_progress` exists, `pre_active_sw_file` is
   re-analysed and, if its `fwver` equals `FWVERSION` in `/etc/system.xml`, it is copied to
   **`/mnt/private/active_sw_file`**, the flag is removed and 2 is returned. On a version mismatch
   ("FW Upgrade is NOT OK") the flag is removed and 3 is returned, which ends in the error screen
   with no image. After a return of 2, `clearSwUpgradeFiles(true)` deletes
   `.Pacemaker/settings.dat` (device settings reset) and any leftover `.pfw` files. [V]
7. `createSystemFileOnPublicVfat` (@0x722b8) rewrites `/mnt/public/.Pacemaker/system.xml` on every boot
   from `/etc/system.xml` with serial, `SWVERSION` (from `/mnt/vf/system.xml`) and bootloader (from
   mtd1). That matches the real dump in `work/device-dump/system.xml`. [V]

**The Editor side.** The Mac Editor's `DeviceCommunication::updateFirmware` (decompiled) copies the file to
`<device root dir> + name + ".pfw"` and waits for the volume to be unmounted. The device root dir is the
parent of a probed file whose names come from the strings `system.xml` and `.Pacemaker`, i.e.
`.Pacemaker/` [I; the field contents were not traced]. This agrees with the device scanning
`/mnt/public/.Pacemaker`, which closes the "copy destination not pinned" gap in FINDINGS.md section 5.

Because the device returns to INIT after USB eject (`masstorageState` ends in state 0 with the "host
already seen" flag set), a `.pfw` dropped over mass storage is picked up on that pass. [V from code, not run]

## 3. When formathdd.sh runs, and what a blank disk does

`HddInterface::formatHdd` (@0x6a1e8) = unmount the loop image, `umount /mnt/public`, `umount
/mnt/private`, `system("formathdd.sh")`. It has exactly two call sites [V]:

- `DebugAction::run` case 2 (the "Restore HDD" action), and
- `DiagAction::hddCheckState` (@0x76ed4, production-test "HDD Check": formats unconditionally, then
  mounts both partitions to verify).

How they are reached:

- **Automatic, with prompts: mount failure.** In `initState`, if `mountPrivateExt3()` returns 1
  (five failed attempts, log "Ext3 mount failed") or `mountPublicVfat()` returns 1 (five failed
  attempts, "Vfat mount failed"), the state machine goes to state 5 = RESTORE_STATE. In `process`,
  state 5 posts message 0xD once. `PwrApplication::handleMessage` (@0x61478) case 0xD shows a modal
  dialog "Sorry friend... / Hard drive needs / checking" with a **Continue** button (language ids
  166-168, 193 from `english.lang`), then `DebugMediator::doAction(2)` shows "Restore hard drive?"
  with **Restore / Cancel** (ids 173, 174, 139); if the first entry (Restore) is chosen it runs
  `formatHdd` ("In progress!"). `doAction(3)` then always unmounts and runs `reboot`.
  A blank or unpartitioned drive fails `mount ... /dev/hda2`, so it ends here. The restore choice index
  (0 = Restore) is [I] from the item order. Nothing formats without the two button presses. [V]
- **Manual: key combos at power-on.** `PwrApplication::PwrApplication` (@0x60e40) checks held keys:
  event keys 2+5+6 launch the **debug menu** (`DebugMediator::showMenu`: Check FS, Clean FS, Restore
  HDD, Reboot); event keys 1+5+6 launch **production-test/diag mode**. Event key ids come from
  `CEventCatcher::KeyToDesc` (@0x99740): 1 = BTN_CHANNEL1, 2 = BTN_CHANNEL2, 5 = BTN_SEARCH_FW,
  6 = BTN_SEARCH_BW. So debug menu = **CHANNEL2 + Forward-search + Back-search** held while booting, diag =
  CHANNEL1 + Fwd + Back [the held-key tests are V; that the juce key codes 0xF629/0xF62A/0xF640/0xF645 are the
  DFB ids used by `DFBKeyToEventKey` and the physical button labels are [I]].
  The DFU combo (Play+Cue+Fwd+Back, FINDINGS section 5) is bootloader level and unrelated.
- **Never** on a normal healthy boot.

Boot with a **mountable but empty hda2** (mkfs'd ext3, no image), FAT32 present [V]:
1. no `swapfile` -> `repairPrivateExt3` creates it (dd 129 MiB, takes a while);
2. `finalizeSwUpgrade` -> 1 (no flag); `mountActiveDmgFile` -> 1, only logs "Could not mount application";
3. `mountPublicVfat` ok, `repairPublicVfat` creates `.Pacemaker/{Music,tmp}` if needed;
4. `prepareSwUpgrade`: a valid `.pfw` in `.Pacemaker/` starts the update (section 2);
5. otherwise, if the charger or USB is attached, the device waits up to 3000 ms for a USB host
   (`UsbIO::isHostConnected`), then enables mass storage (`modprobe g_file_storage
   file=/dev/hda1 removable=1 stall=0 maxpwr=500`, screen "Do not disconnect! / Before pulling the
   plug / eject from computer"). This happens even though no image exists;
6. with no host or no cable, `isActiveDmgFileOk()` is false so message 6 is shown ("Hard drive restored /
   Please upgrade firmware", ids 187-189) in the error state; attaching charger or USB later returns the
   machine to INIT (`showErrorState`), which repeats step 5. No reboot loop, no format.

Boot with an **unmountable hda2 or hda1** -> state 5 restore dialogs as above, no image launched.

## 4. Recipe for a blank SSD

### Required on-disk facts

From `fdisk_hdd` + `formathdd.sh` (identical in 14142 and 180096069 rootfs) [V]:

    fdisk /dev/hda < fdisk_hdd        # d 1; d 2; d 3; d 4; n p 2 <default> +500M; n p 1 <default> <default>; t 1 c; w
    mkdosfs -F 32 -n Pacemaker /dev/hda1
    mkfs -t ext3 /dev/hda2            # no label, no options (e2fsprogs 1.34, so 128-byte inodes [I])

- MBR (DOS) partition table, **two** partitions, addressed by number: `/dev/hda1` FAT32 and
  `/dev/hda2` ext3.
- hda1: type **0x0C** (W95 FAT32 LBA), takes the rest of the disk, label **Pacemaker**
  (cosmetic for the device, which mounts by node [V]; the Editor's use of the label is unchecked).
- hda2: default type **0x83**, **500 MiB** (`+500M`).
- Physical order: because partition 2 is created first at the default start and partition 1 afterwards,
  **hda2 sits first on the disk and hda1 follows** [I from the fdisk keystrokes; the device addresses
  both by number so the order is not functionally checked anywhere]. If exact fidelity matters, compare
  with the partition table of the old drive (the owner can read it without risk: `Get-Partition` /
  `diskpart list partition` on a read-only attach; I did not touch drive I:).
- Kernel 2.6.19.2 on the device, `mxc_ide`, `hdparm -d 1` / `-X udma5` / `-S0` are issued by the
  scripts.

### Method A: prepare everything on a PC (device boots without upgrading)

1. Create the MBR with the two partitions above on the SSD (any partitioning tool; on a PC prefer to
   keep hda2 first as the factory layout does [I]).
2. hda1: FAT32, label `Pacemaker`. Create `.Pacemaker\`, `.Pacemaker\Music\`, `.Pacemaker\tmp\`
   (optional, the device recreates them [V]). Copy the user's library back (`music.db`, `Music\`,
   `settings.dat`) from the backup.
3. hda2: ext3 (not ext4). Put the file `active_sw_file` in the **root** of the filesystem,
   byte-copied from `vendor/firmware/dfw_180096069.pfw` (including the 512-byte trailer, as the
   device itself copies it with `copyFileTo`). Nothing else is required: `swapfile` is created at
   first boot [V]. No `upgrade_in_progress`.
   Result at boot: `finalizeSwUpgrade` -> 1, `mountActiveDmgFile` mounts `/mnt/vf`, pmplayer starts, and
   `settings.dat` is **not** wiped (it is only wiped after an install via Method B) [I].
4. ext3 creation caution [H]: the device kernel is 2.6.19.2 and the factory mke2fs is 1.34.
   A current `mkfs.ext3` may enable features that kernel cannot mount. Use `-t ext3` (not ext4) and, to
   stay close to the factory result, `-I 128`; afterwards check `dumpe2fs -h` shows only
   `has_journal ext_attr resize_inode dir_index filetype sparse_super large_file`. A kernel refuses a
   filesystem with unknown incompat features. This is general ext3 knowledge, not verified on the
   device. Windows cannot write ext3 natively, so do this step on Linux (or a VM).
   The image inside `active_sw_file` is unaffected (it is mounted via loop, read-only).
5. Untested size caution [H]: capacity limits of `mxc_ide` / the 2.6.19 IDE driver (for example above 128 GiB
   / LBA48) and of a PATA-to-SSD adapter are not evidenced anywhere in this analysis. Check the old
   drive's capacity (not read by me) and, if in doubt, stay at or below it.

### Method B: partitions only, let the device install the firmware (needs no ext3 write on the PC, resets settings)

1. Same MBR and FAT32 as above. hda2 can be a freshly made **empty** ext3 (no files).
2. Put `dfw_180096069.pfw` in `hda1:\.Pacemaker\` (any name). Must have about 49 MiB free on hda1.
3. Boot. Flow: swapfile created, `prepareSwUpgrade` moves the file to hda2, flag set, "Firmware upgrade
   in progress", no flash writes because `FWVERSION` 180096069 equals the running rootfs and the
   bootloader value 12838 equals mtd1 (the real dump shows both), reboot, `finalizeSwUpgrade` copies it
   to `active_sw_file`, `settings.dat` is deleted. [V code path; hardware untested]
   Using a different version's `.pfw` would instead flash kernel/rootfs to that version (section 2 point 5):
   avoid unless intended.

### Method C: completely blank / unpartitioned SSD, device does the layout itself

1. Fit the SSD. Boot. Both mounts fail, state 5: press Continue, then Restore (section 3).
   `formathdd.sh` repartitions with the same `fdisk_hdd` and runs `mkdosfs`/`mkfs`, then the device reboots.
   Whether util-linux `fdisk` copes with the leading `d` keystrokes on a disk with no table is not
   verified here [H]; the same script is the factory/production path (`DiagAction::hddCheckState`), so it
   is expected to work.
2. After the reboot the screen says "Hard drive restored / Please upgrade firmware". Connect USB (and/or
   the charger; one of them is required for the mass-storage branch). The device exports
   `/dev/hda1`; copy the `.pfw` into `.Pacemaker\` (the folder already exists, step 3 of the blank
   boot flow), eject. The device installs it as in Method B.

### Alternative: image the old drive

`active_sw_file`, `pre_active_sw_file`, `swapfile` and the flags on hda2 are all reproducible (the first from
the `.pfw`). The only irreplaceable data is on hda1. So the reasonable choices are (i) file-level copy of
hda1 to the new FAT32 (the existing backup) plus Method A/B, or (ii) a whole-disk raw clone
(MBR + both partitions) to an SSD at least as large as the source. A raw clone of a failing disk copies
the same hda2 contents (including any corruption on hda2) so (i) is cleaner. If a clone's hda2 fails to
mount the device simply offers the Restore dialog and nothing is lost on hda1 until Restore is confirmed
(formatHdd wipes both partitions), so keep a backup first.

## Open points

- The device-side key labels for CHANNEL1/CHANNEL2/Forward/Back versus the physical buttons are not checked.
- Whether the old drive's hda2 really has partition order hda2-first: inferred, see above.
- `runModal()` index to button mapping in the restore confirmation: inferred from item order.
- Nothing here has been run on hardware; the first boot of a prepared SSD is the real test, and it is
  safe in the sense that no step formats anything unless Restore is pressed.
