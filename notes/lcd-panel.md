# Pacemaker LCD panel: controller, interface, replacement guidance

Evidence tags: [V] verified from source/binary/datasheet this session, [I] inferred, [H] hypothesis / unverified.

## Answer

The panel controller is a **Himax HX8347-A (A01)**, 240RGB x 320, 262k colours, internal GRAM. The Tonium module is called **UMNH8075** (the kernel driver is `mxcfb_umnh8075`; the "urt7952" in log lines is only a leftover printk prefix macro). Confidence: high (about 95%).

## Evidence

1. [V] Tonium published the kernel driver source (GPL): `work/gpl/pmib/pace/linux/ltib-imx31ads-20070511/pacemaker.drivers.video.mxc.patch`, file `drivers/video/mxc/umnh8075.c` (driver v1.6, Tonium AB 2008-09, built Jan 2010). `Kconfig`: "UMNH8075 Panel". Ghidra was not needed.
2. [V] The stock kernel `work/build/vmlinux-stock.bin` contains the same driver built in: the string `mxcfb_umnh8075` (va 0xc0264fee), the `[urt7952 | ERROR]` strings, and the init function at va ~0xc013f6xx..0xc013fbxx. The compiled register writes (helper `reg_write` = 0xc013efec, `r0=reg, r1=value`) match the source in order, e.g. `0x16<-0xC8`, `0x23<-0x95`, `0x24<-0x95`, `0x25<-0xFF`, `0x3C<-0xF0`, `0x35<-0x38`, `0x36<-0x78`, `0x19<-0x49`, `0x93<-0x0F`, `0x20<-0x40`, `0x1D<-7`, `0x1F<-4`, `0x44<-0x40`, `0x45<-0x12`, and a `cmp r0,#0x47` after the ID read at 0xc013fbec. So the GPL source describes the shipped kernel.
3. [V] The driver reads **register 0x67 and requires 0x47** ("Verify that we have the correct display by checking the 'Himax ID code'"), then reads back R01 expecting 0x06. HX8347-A01 datasheet (Version 02, Nov 2007) register table: `R67h Himax ID code, R, HimaxID[7:0] (0100_0111)` = 0x47. [V]
4. [V] Register names and addresses used by the driver match the HX8347-A01 map: R01 Display Mode (IDMON/INVON/NORON/PTLON), R02-R09 column/row window, R16 Memory Access (MY/MX/MV/BGR), R19 OSC (CADJ), R1B Power Control 1 (PON/DK/STB), R1C-R20 power controls (AP, VC1, VRH, BT), R22 SRAM write, R23-R25 cycle control (N_DC/PI_DC/I_DC), R26 Display Control 1 (GON/DTE/D), R27-R2D porches, R35/R36 EQS/EQP, R38 RGB interface control, R3A-R3C cycle control (RTN/NW), R43-R46 SON/GDON/GDOF, R90 SAP, R93 OSC RADJ. (The driver comments call 0x24 "PI_DC", the datasheet table lists it as I_DC; cosmetic.) Datasheet text was extracted from a copy of the Himax HX8347-A01 datasheet (focuslcds.com mirror); a Microchip ASF header page was unreachable.
5. [V] Datasheet interface facts: register-content mode supports 8/16/18-bit 80/68-system parallel, serial, and 16/18-bit RGB; IOVCC 1.65-3.3 V, VCI 2.3-3.3 V; 8-bit register index style (not 16-bit like ILI932x, not MIPI DCS).
6. [I] HX8347-A is a different family from HX8347-D/G (different register map, ID in R00h/0x67 values differ per variant). A web hit says -A reads 0x4747 at 0x67 in 16-bit reads, consistent with 0x47 in the low byte [unverified source: Arduino forum thread].

## Interface (all [V] from source unless noted)

- Bus: i.MX31 IPU ADC, display port DISP0, `IPU_ADC_IFC_MODE_SYS80_TYPE2` (8080-style, separate WR/RD, RS), `ifc_width` default **18** (module parameter `mxcfb_umnh8075.ifc_width` can override; code comment says 8 or 18 are the valid values), `IPU_ADC_BURST_WCS`, `IPU_ADC_SER_NO_RW`.
- Pixel format on the bus: **RGB666** (`pix_fmt = IPU_PIX_FMT_RGB666`, 18 data lines LD0..LD17). Framebuffer is ABGR32 in memory, converted by the IPU.
- Control pins requested by `slcd_gpio_config`: LD0..LD17 data, WRITE (NWR), PAR_RS (RS/DNC), LCS0 (chip select). A READ strobe must exist because the driver reads R67 [I; READ pin not seen in the patch hunk].
- Resolution 240x320 (`MXCFB_SCREEN_WIDTH/HEIGHT`), no offsets.
- GPIOs (i.MX31 CSI pins used as plain GPIO): **CSI_D9 = panel reset** (driven low, 5 ms, then high; 65 ms wait before init), **CSI_D11 = "LCD_ENABLE / RGB-enable"** (held 0 = RGB interface disabled, panel used in MPU mode), CSI_D13 = "NCS pin", set as input [I: direction value 1 = input in the mxc API].
- Timing (`ipu_adc_init_ifc_timing`, ns; argument meaning per Freescale BSP [I]): during init, slow: read cycle 630, up 90, down 590, latch 540, 5 MHz pixel clk; write cycle 110, up 12, down 49. Run time: write cycle **100 ns**, up 35, down 70 (i.e. about 35 ns WR low, 10 MHz write rate); read timing unchanged slow.
- Backlight: not on the panel controller. MC13783 PMIC LED driver: `pmic_bklit_*`, LED1, current 7 and duty 15 on, 0 off; those are the LIGHT+/LIGHT- pads. Default brightness 75.
- GRAM write: window registers R02/R03 (col start hi/lo), R04/R05 (col end), R06/R07 (row start), R08/R09 (row end), then index R22 and pixel data. The IPU "template" writes: cmd 0x06 + Y addr hi, 0x07 + Y addr lo, 0x03 + X addr, 0x22 then data stream.

## Init sequence (all values hex, in order) [V]

Hard reset (CSI_D9 low 5 ms, high), wait 65 ms, then:

1. Window: R38=00 (RGB signal polarities); R02=00 R03=00 (col start 0); R06=00 R07=00 (row start 0); R04=00 R05=EF (col end 239); R08=01 R09=3F (row end 319).
2. Display setting: R01=06; R16=C8 (ML=1, BGR=0); R23=95 R24=95 R25=FF; R27..R2D (except R2B/R2E): 02 each (porches); R3A=01 R3B=01 R3C=F0 R3D=00; wait 20 ms; R35=38 R36=78 R3E=38 R40=0F R41=F0.
3. Power: R19=49 (OSC); R93=0F (RADJ); wait 10 ms; R20=40 (BT); R1D=07 R1E=00 R1F=04; VCOM: R44=40 R45=12; wait 10; R1C=04 (AP); wait 20; R43=80 (VCOMG); wait 5; R1B=08; wait 40; R1B=10 (PON=1); wait 40.
4. Display on: R90=7F (SAP); R26=04; wait 40; R26=24; R26=2C; wait 40; R26=3C (GON/DTE/D=11).
5. Internal VDDD: R57=02 (test mode), R55=00 (VDDD 1.95 V), R57=00.
6. Partial/scroll regs: R0A=00 R0B=00 R0C=00 R0D=EF R0E=00 R0F=00 R10=01 R11=3F.
7. Verify: read R67 == 0x47, read R01 == 0x06 (result is ignored by the caller; no fatal fallback).
- Display off (suspend): R01=08, backlight 0, IPU clock off.

Note: no gamma registers (R46-R51 area) are programmed; the controller default gamma is used. [V]

## Matching alternatives (why not others)

- HX8347-A is **not** compatible at command level with ILI9341, ST7789, ILI932x, SSD1289 etc. (different index width, window registers, init). The driver issues R22 SRAM write with 8-bit indexes and Himax-specific power regs. [V/I]
- HX8347-D (ID 0x95 at R00, different power registers, used in many cheap 2.4" Arduino shields) is related but not binary compatible with this init: the power/gamma block differs. Chance a D works with this driver unchanged: low (hypothesis [H]; it might show an image if the A-style regs happen to alias, but do not count on it).

## Connector (photo `5df97604-image.jpg`) [I, all estimates]

- The large white ZIF (flip-lock, flex enters from the left, orange flex with brown stiffener) has about **40 contacts**: counted about 40 solder tails along the PCB side in a 2x zoom (tail spacing about 40 px over about 1610 px). Range: 38-42, 40 most likely.
- Pitch: **0.5 mm** (assumed; typical for 40-pin FPC of this width, a 0.3 mm pitch would have needed ~65 pins). Contact side (top/bottom) is not determinable from the photo; check the flex.
- Silkscreen numbers 1..27 are visible along the pad row; "LIGHT+" and "LIGHT-" pads sit next to it. The backlight is probably separate wires/pads (LIGHT+/-) rather than flex pins [H].
- The second, smaller ZIF at bottom centre (flex with "05B" marking) is a different connector (about 24 contacts at 0.5 mm?) probably keypad/touch [H], not the display.
- Pin count check: HX8347 18-bit 8080 needs D0-D17 (18) + NCS, RS, NWR, NRD, RESET (5) + IOVCC, VCI, GND(s) (about 4-5) + backlight A/K (2) = about 29-31 minimum; 40 is plausible with extra grounds, ID pins and the unused RGB-interface lines (VSYNC/HSYNC/DOTCLK/ENABLE) [I].

## What a replacement needs

Hard requirements: Himax HX8347-A (or A01), parallel 18-bit 8080 interface in MPU mode (BS pins strapped to 18-bit 80-system on the module), 240x320, 3.3 V-class logic (IOVCC and VCI between about 2.3-3.3 V), white LED backlight that the MC13783 LED driver can drive (original LED string forward voltage unknown, measure LIGHT+/LIGHT- on the original panel first), and a flex that mates with a 40-pin 0.5 mm ZIF with the Tonium pinout (pinout unknown, obtainable from the board traces or by finding the original module's pin table). An IPS panel with HX8347-A is unlikely to exist; I found no IPS HX8347-A part in this session [H].

Practical options, easiest first:
1. Source an original/new-old-stock Pacemaker (Tonium PM-1 / "Pacemaker" DJ player) LCD, or the module marked **UMNH8075** (marking may be on the flex or glass edge; verify the printed model).
2. Buy any 2.4" 240x320 TFT with **HX8347-A**/HX8347-A01 and 18-bit parallel interface (older Newhaven/Ampire/focus-style modules; datasheet at Newhaven "HX8347" support page) and build a tiny flex-to-flex adapter board for the pin mapping, then verify the R67h read returns 0x47.
3. Use a modern IPS panel (ILI9341 / ST7789 class with 8/16/18-bit parallel mode, e.g. a 2.4" 240x320 IPS with an 8080 18-bit or 16-bit interface) and **change the kernel driver**: the init is in the built-in `umnh8075.c` (GPL source available), so replacing `_init_panel_umnh8075()` and the window/GRAM template (R02..R09 / R22 become 0x2A/0x2B/0x2C DCS) is a kernel rebuild/patch; it needs flashing the kernel (mtd2), and a 16-bit bus mode (`ifc_width=16`) may be needed because most IPS modules expose 16 bits. This is a software project, not a drop-in swap. Also the module must tolerate the 100 ns write cycle (fine for ILI9341/ST7789).

Search terms: "HX8347-A 2.4 inch 240x320 TFT 18-bit parallel", "HX8347A01 TFT module 40 pin 0.5mm FPC", "UMNH8075", "Tonium Pacemaker LCD replacement", "Pacemaker DJ player display screen". For the IPS route: "2.4 inch IPS 240x320 8080 parallel 18-bit 40pin FPC ST7789".

## Open questions / how to confirm without risk

1. Read the part marking on the display flex/glass of the unit (UMNH8075 / printed rev) [H].
2. Trace LD0..LD17, NWR, NRD, RS, NCS, RESET on the ZIF with a multimeter against the i.MX31 BGA to get the exact pinout [H].
3. Measure LIGHT+/- voltage/current with the stock panel to size a replacement backlight [H].
