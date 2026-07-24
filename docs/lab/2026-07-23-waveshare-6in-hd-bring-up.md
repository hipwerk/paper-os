# Waveshare 6-inch HD bring-up — 2026-07-23–24

## Fixture

- Host: Raspberry Pi 3 Model B Rev 1.2, Raspberry Pi OS Lite 64-bit
- Panel: Waveshare 6-inch HD, 1448×1072, panel marking ED060KC1
- Controller: IT8951 Driver HAT (B) with 6inch e-Paper Adapter (B)
- Panel profile: `desk-6in-hd`
- FPC VCOM label: −2.08 V; configured magnitude: 2080 mV
- SPI: `/dev/spidev0.0`, mode 0, manual GPIO8 CS, 1 MHz
- GPIO: reset 17, HRDY 24
- Probed firmware: `SWv_0.6.`
- Probed LUT: `M841_TFAB512`
- Probed image buffer: `0x00122480`
- Reviewed Waveshare source: commit `8640693`

## Controller and power bring-up

The read-only PaperOS probe successfully reset and identified the IT8951
controller. The first one-second HRDY budget expired; a bounded five-second
budget was sufficient for this fixture. Every observed reset restored the
controller VCOM register to 2800 mV. Before each attempted PaperOS refresh, the
diagnostic applied the panel-label magnitude of 2080 mV and verified immediate
readback.

The first assembled power-on mistakenly supplied power through the HAT USB
connector and produced one Raspberry Pi undervoltage/throttling record. Power
was removed cleanly and moved to the Raspberry Pi power input, leaving the HAT
USB connector empty. Subsequent checks reported `throttled=0x0`, with
temperatures around 44–47 °C.

These checks proved communication with the powered controller board. They did
not prove that the raw panel glass was connected.

## No-motion observations

Several supervised commands completed without controller or transport errors,
but the operator observed no flash or visible glass change:

1. PaperOS release `/opt/paperos/releases/d44f27cb4632c99c`, using a streamed
   multiword Gray4 upload and explicit-buffer refresh command.
2. PaperOS release `/opt/paperos/releases/9e90d455b8165108`, temporarily changed
   to per-word uploads and current-buffer refresh while investigating.
3. A minimal executable built from the reviewed Waveshare driver.
4. Waveshare's unchanged full demo.

Both PaperOS paths matched pinned controller identity, applied 2080 mV with
verified readback, and completed their guarded sequences. The vendor programs
also communicated with the controller but produced no visible panel response.
Because independent implementations failed in the same way, these observations
did not establish a defect in either PaperOS transfer path.

## Root cause

After powering down, inspection found that the panel's orange/brown 34-contact
FPC was inserted into the 6inch e-Paper Adapter (B) in the wrong orientation.
The gray 40-conductor HAT-to-adapter ribbon was correctly seated.

For this exact adapter and panel batch, with the adapter's printed blue PCB side
facing up, the `-2.08` label on the orange FPC faces the same direction as the
blue stiffener at the adapter end of the gray ribbon. The adapter connector's
two dark edge locks must be released and closed evenly. This is recorded fixture
evidence, not a universal color-orientation rule.

The controller remained independently reachable while the panel FPC was
reversed. That explains why identity, VCOM, upload, and display-status checks
could all succeed without any motion on the glass.

## Independent hardware baseline: passed

With power removed, the orange FPC was corrected and locked. The Raspberry Pi
was powered again and the same unchanged Waveshare full demo was run with the
exact factory VCOM:

```sh
sudo ./epd -2.08 0
```

The panel visibly rendered the demo sequence and completed its final white
cleanup. Post-run Raspberry Pi health remained `throttled=0x0`, at approximately
45 °C. This establishes that the Pi, HAT, adapter, panel, power path, VCOM label,
and vendor driver work together.

## Conclusions

- The actual no-motion cause was the reversed panel FPC.
- The temporary PaperOS per-word/current-buffer change was based on symptoms
  caused by that wiring fault and has no supporting protocol evidence.
- The original PaperOS streamed upload and explicit-buffer refresh remain the
  intended implementation and are covered by fake-transport protocol tests.
- A red HAT LED and successful IT8951 probe prove board power/controller
  communication only; the raw glass has no enumerable identity or status LED.

## PaperOS physical calibration: passed

The original streamed upload and explicit-buffer refresh were restored. The
complete repository gate passed with 78 tests and 87.87% workspace line
coverage. Release `/opt/paperos/releases/c5fadfbd1b7c8e99` was deployed and its
hardware-free remote smoke tests passed.

The first calibration attempt after the vendor demo aborted during probe with
an invalid panel-size response. The previous PaperOS release produced the same
read-only failure, excluding the new build as the cause. No VCOM write or
refresh occurred. After a clean shutdown and ten-second power removal, probe
again returned the pinned identity:

- size: 1448×1072
- image buffer: `0x00122480`
- firmware: `SWv_0.6.`
- LUT: `M841_TFAB512`
- reset VCOM: 2800 mV

One supervised PaperOS calibration then:

1. applied the profile's 2080 mV VCOM and verified readback;
2. performed white INIT and the streamed packed-Gray4 GC16 refresh;
3. slept for a 30-second visual hold;
4. reprobed the same identity after wake;
5. reapplied 2080 mV with verified readback;
6. completed white INIT cleanup and slept.

The operator observed and photographed the expected page: a black one-pixel
border reaching all four panel edges, a narrow repeating `0/5/10/15` packing
strip, and a smooth 16-level grayscale ramp. Because the native landscape
framebuffer was viewed with the panel in portrait orientation, the middle-half
ramp appeared as a central vertical column and the packing strip as a narrow
left column; the remaining white areas were intentional. The panel returned to
white after the hold.

Post-run temperature was 46.2 °C. `get_throttled` was `0x50000`, recording a
brief historical undervoltage/throttling event during boot, with no current
undervoltage or throttling bits set during calibration.

This satisfies the first pixels-on-glass exit gate: full-panel addressing,
Gray4 packing and nibble order, explicit-buffer refresh, grayscale waveform,
session VCOM handling, cleanup, and bounded failure behavior are now verified
on the named fixture.

## PaperOS typography specimen: passed

The deterministic typography slice passed the complete repository gate with 84
tests and 88.35% workspace line coverage. The same accepted 1072×1448 Gray8
golden page used by desktop preview was cross-compiled and deployed as release
`/opt/paperos/releases/1e5c7b7d553b7884`. Device-free Pi smoke tests rendered
both preview applications before activation and did not open SPI or GPIO.

The named Pi profile was updated with `rotation_degrees = 90`, mapping the
logical portrait page to the controller's native 1448×1072 landscape buffer.
One separately authorized specimen run then:

1. probed the pinned 1448×1072 controller, `SWv_0.6.` firmware,
   `M841_TFAB512` LUT, and `0x00122480` image buffer;
2. observed the reset VCOM of 2800 mV;
3. applied the FPC value of 2080 mV and verified readback;
4. performed white initialization and streamed the canonical Gray8 specimen as
   controller-order Gray4 using GC16;
5. slept while leaving the specimen visible for a ten-second review;
6. reprobed the same identity, reapplied 2080 mV, and verified readback;
7. completed white cleanup and slept.

The operator confirmed correct portrait output, complete content, and beautiful
print-quality typography on the physical panel. The display returned to white
after the review. This satisfies the typography-and-preview exit gate: bundled
font shaping, scene rasterization, grayscale composition, rotation, streamed
Gray8 conversion, and the desktop-to-glass pixel path are verified together on
the named fixture.
