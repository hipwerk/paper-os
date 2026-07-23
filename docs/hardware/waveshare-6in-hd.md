# Waveshare 6-inch HD E-Paper HAT lab guide

Target panel: 1448×1072 monochrome/16-gray panel with IT8951 controller.

## Safety

- The glass, FPC, and connector are fragile. Reinforce/support the FPC during
  bench work.
- Connect all panel and HAT cables before power. The board is not hot-pluggable.
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
- The implemented physical backend is deliberately full-screen only: packed
  Gray4 INIT/GC16. Direct Gray8, partial, and A2 profiles are not advertised.
- HRDY and display-engine polling share the wall-clock deadline from the named
  panel profile.
- Panel size, image-buffer address, firmware, and LUT are probed rather than
  hard-coded.
- Explicit VCOM writes are read back and fail on any mismatch.

The physical width 1448 is not divisible by 32. Fast 1-bpp behavior at the last
eight columns must be measured and represented as a mode-specific addressable
bound; the runtime must fall back to grayscale rather than drop changed pixels.

## Local panel profile

Copy `hardware/panels.example.toml` to `hardware/panels.local.toml` (ignored by
Git) and fill it from the physical unit. Give each panel a stable local name.
Never select the first detected device when a test can alter VCOM or refresh.
Verify the SPI node, GPIO chip, CS/reset/ready offsets, timing budgets, native
dimensions, and FPC VCOM; zero or duplicate safety-critical values are rejected.

## Bring-up sequence

1. Photograph/record the panel model, HAT revision, FPC VCOM, Pi model, OS/kernel,
   wiring, and DIP position.
2. Enable SPI and run a loopback test before attaching the HAT if wiring is in
   doubt.
3. Run the vendor demo once with the exact VCOM. Record its device-info output
   and confirm the panel itself renders correctly.
4. Run PaperOS probe-only mode:

   ```sh
   paperos-hardware probe \
     --config hardware/panels.local.toml \
     --profile desk-6in-hd \
     --allow-hardware
   ```

   It resets, wakes, reads identity and VCOM, never writes VCOM, verifies the
   named profile, and sleeps. Compare panel size, memory address, firmware, LUT,
   and current VCOM to the baseline.
5. With separate authorization, run:

   ```sh
   paperos-hardware calibrate \
     --config hardware/panels.local.toml \
     --profile desk-6in-hd \
     --allow-hardware \
     --allow-refresh
   ```

   It runs white INIT and a packed Gray4 GC16 calibration page, sleeps during
   the bounded observation interval, then resets/reprobes/revalidates identity
   and VCOM before white INIT cleanup and sleep. `SIGINT` or `SIGTERM` requests
   graceful sleep rather than terminating immediately.
6. Add partial and A2 experiments only after the full path is stable.

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
