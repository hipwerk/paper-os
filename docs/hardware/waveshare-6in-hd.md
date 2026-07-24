# Waveshare 6-inch HD E-Paper HAT lab guide

Target panel: 1448×1072 monochrome/16-gray panel with IT8951 controller.

## Safety

- The glass, FPC, and connector are fragile. Reinforce/support the FPC during
  bench work.
- Connect all panel and HAT cables before power. The board is not hot-pluggable.
- Photograph the pin-1 ends of both the 40-pin gray ribbon and 34-pin panel
  adapter connection before power. Do not infer contact orientation only from
  the color of a stiffener; follow connector pin markings and the official
  connection photograph.
- Supply the HAT with 5 V as documented; GPIO signaling remains through the HAT.
- Every panel has a different negative VCOM printed on its FPC. Record the
  positive millivolt magnitude in the local profile (for example, `-1.50 V`
  becomes `1500`). Never copy an example, infer it from model, or commit a
  guessed default. Repeated operation at the wrong VCOM degrades the display.
- Full refresh visibly flashes. That is normal. Stop if output is blurred,
  unexpectedly colored, or the controller repeatedly times out.
- Respect the documented 0–50 °C operating range. Log ambient temperature
  because waveform behavior and ghosting are temperature-sensitive.

## SPI wiring

When the HAT is not directly stacked, Waveshare documents:

| HAT | Raspberry Pi BCM | Purpose |
| --- | ---: | --- |
| 5V | 5V | power |
| GND | GND | ground |
| MISO | 9 | SPI MISO |
| MOSI | 10 | SPI MOSI |
| SCK | 11 | SPI clock |
| CS | 8 | active-low chip select |
| RST | 17 | active-low reset |
| HRDY | 24 | low while controller is busy |

Set the HAT DIP switch to SPI before power. Treat this table as a starting
fixture profile and verify against the board revision and `gpioinfo`.

PaperOS controls CS through GPIO rather than native spidev chip select. The
IT8951 requires CS to remain low while HRDY is sampled after its SPI preamble.
Configure `dtoverlay=spi0-0cs`, retain `/dev/spidev0.0` for clock/data, and
include `cs_line` in the named panel profile.

## Controller facts represented in code

- INIT/cleanup is mode 0.
- GC16 quality grayscale is mode 2.
- Fast A2 mode is allowlisted by firmware/LUT: M641 uses mode 4; documented M841
  families use mode 6. Unknown LUTs do not advertise fast refresh.
- On 6-inch M641 and M841_TFAB512 firmware, 1-bpp update X and width require
  four-byte (32-pixel) alignment.
- Pixel buffers support 1/2/4/8-bpp; endian/packing is part of the controller
  adapter, not application rendering.
- PaperOS Gray4 bytes put the left pixel in the high nibble. IT8951 packed-write
  words put the first pixel in the low nibble, so the backend reverses the four
  nibbles of each uploaded word. Protocol vectors test `01 23` → `3210`.
- The implemented physical backend is deliberately full-screen only: packed
  Gray4 and canonical Gray8 INIT/GC16. Gray8 is quantized and streamed as
  controller-order Gray4 without an intermediate packed frame. Partial and A2
  profiles are not advertised.
- HRDY and display-engine polling share the wall-clock deadline from the named
  panel profile.
- Panel size, image-buffer address, firmware, and LUT are probed rather than
  hard-coded.
- Explicit VCOM writes are read back and fail on any mismatch.

The HAT's red LED establishes only that the controller board has 5 V power. A
successful IT8951 identity/VCOM probe establishes communication with the
controller. Neither proves that the raw e-paper glass is electrically connected:
the panel has no enumerable identity and no status LED.

The physical width 1448 is not divisible by 32. Fast 1-bpp behavior at the last
eight columns must be measured and represented as a mode-specific addressable
bound; the runtime must fall back to grayscale rather than drop changed pixels.

## Local panel profile

Copy `hardware/panels.example.toml` to `hardware/panels.local.toml` (ignored by
Git) and fill it from the physical unit. Give each panel a stable local name.
Never select the first detected device when a test can alter VCOM or refresh.
Verify the SPI node, GPIO chip, CS/reset/ready offsets, timing budgets, native
dimensions, and FPC VCOM; zero or duplicate safety-critical values are rejected.
Start `max_spi_hz` at 1 MHz. Profiles reject values above 12.5 MHz, the rate
audited in Waveshare's Raspberry Pi implementation.
Set `rotation_degrees` to the clockwise right-angle transform that maps the
application's logical page to the controller's native dimensions. The recorded
portrait fixture uses `90`.

## Cable assembly for the recorded fixture

The recorded fixture uses a Driver HAT (B), 6inch e-Paper Adapter (B), and an
ED060KC1 panel. With all power disconnected:

1. Seat the gray 40-conductor ribbon between the HAT connector marked `40` and
   the adapter connector marked `40`; lock both connector tabs.
2. Seat the panel's orange/brown 34-contact flexible cable in the adapter
   connector marked `34`; lock both dark edge tabs evenly.
3. On this exact adapter/panel batch, when the adapter's printed blue PCB side
   faces up, the `-2.08` label on the orange panel cable faces the same direction
   as the blue stiffener at the adapter end of the gray ribbon.
4. Compare the complete assembly with
   [Waveshare's connection photograph](https://www.waveshare.com/w/upload/a/a0/Faqe-link.png).

That orientation is fixture evidence, not a universal cable-color rule. If the
board revision, panel, or adapter differs, use its pin markings and official
documentation. Never reverse, reseat, or hot-plug either ribbon while powered.

## Bring-up sequence

1. Photograph/record the panel model, HAT revision, FPC VCOM, Pi model, OS/kernel,
   wiring, both connector orientations, pin markings, and DIP position.
2. Enable SPI. If wiring is in doubt, power down and run a loopback test before
   attaching the HAT.
3. Establish the independent hardware baseline with Waveshare's unmodified
   Raspberry Pi demo at the
   [reviewed upstream commit](https://github.com/waveshareteam/IT8951-ePaper/tree/86406933d8f22af9fd3f2152b4958610c054b9a8).
   Build it as documented, then supply the exact negative VCOM printed on the
   attached panel FPC:

   ```sh
   sudo ./epd -2.08 0
   ```

   Replace `-2.08` only with the value printed on the physical panel. Observe a
   complete visible demo and final cleanup. If controller commands work but the
   glass never changes, stop and power down; Waveshare identifies the
   HAT/adapter/panel connection as the likely fault. Recheck both ribbon
   orientations and locks before changing protocol code.
4. Run PaperOS probe-only mode:

   ```sh
   paperos-hardware probe \
     --config hardware/panels.local.toml \
     --profile desk-6in-hd \
     --allow-hardware
   ```

   It resets, wakes, reads identity and VCOM, never writes VCOM, prints the
   report, verifies controller identity against the named profile, and sleeps.
   A boot VCOM different from the profile is reported but is not a probe error.
   Copy the exact printed firmware and LUT strings into `expected_firmware` and
   `expected_lut`. Never copy the controller boot VCOM over the FPC value.
5. Only after identity is pinned, and with separate VCOM-write and refresh
   authorization, run:

   ```sh
   paperos-hardware calibrate \
     --config hardware/panels.local.toml \
     --profile desk-6in-hd \
     --allow-hardware \
     --allow-vcom-write \
     --allow-refresh
   ```

   The target comes only from `vcom_mv` recorded from the FPC. Calibration
   resets, verifies pinned identity, applies the profile VCOM if reset restored
   another value, verifies readback, then runs white INIT and a packed Gray4
   GC16 calibration page with an order-sensitive `0/5/10/15` stripe. After the
   bounded observation sleep it repeats reset, identity verification, VCOM
   application, and readback before white INIT cleanup and final sleep. Any
   identity or readback mismatch aborts before refresh.
6. Render the accepted typography page only with separate authorization:

   ```sh
   paperos-hardware specimen \
     --config hardware/panels.local.toml \
     --profile desk-6in-hd \
     --allow-hardware \
     --allow-vcom-write \
     --allow-refresh
   ```

   The command renders in logical portrait, applies the profile rotation,
   performs a full GC16 update, holds for the bounded observation interval,
   then repeats identity/VCOM verification before white INIT cleanup and sleep.
7. Add partial and A2 experiments only after the full path is stable.

Do not start with a complex Daily page: calibration bars, one-pixel borders,
corner labels, checkerboards, and a typography specimen make orientation,
packing, stride, clipping, and grayscale errors diagnosable.

## What to capture

Keep lab reports under `docs/lab/` without local secrets. Include git commit,
panel profile name, firmware/LUT, VCOM magnitude, SPI clock, regions/modes,
timings, temperature, failures, cleanup behavior, and photographs when visual
quality is the claim.

## Authoritative references

- [Waveshare 6-inch HD E-Paper HAT wiki](https://www.waveshare.com/wiki/6inch_HD_e-Paper_HAT)
- [Reviewed Waveshare demo source at commit 8640693](https://github.com/waveshareteam/IT8951-ePaper/tree/86406933d8f22af9fd3f2152b4958610c054b9a8)
- IT8951 I80/SPI/I2C programming guide linked by Waveshare
- Raspberry Pi SPI/spidev and GPIO character-device documentation

Vendor example clock-divider values are historical and Pi-model-specific.
Start conservatively, use Linux APIs, verify transfers, then benchmark.
