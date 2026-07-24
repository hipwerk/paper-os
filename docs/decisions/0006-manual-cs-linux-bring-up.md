# ADR 0006: Manual-CS Linux IT8951 bring-up

- Status: accepted
- Date: 2026-07-23

The initial direct-Gray8 limitation below is superseded by
[ADR 0007](0007-deterministic-typography-pipeline.md); the manual-CS and
bring-up safety decisions remain current.

## Context

The IT8951 SPI transaction sends a preamble, waits for HRDY, and then exchanges
the command or data while chip select remains asserted. Separate userspace
spidev calls normally release hardware CS, while one `SPI_IOC_MESSAGE` cannot
conditionally pause for GPIO polling between its transfers. Full packed
framebuffer upload also needs a multiword transaction rather than one preamble
per word.

Physical commands must not guess a panel, VCOM, or unbounded wait policy.

## Decision

- Linux spidev runs in mode 0 with `SPI_NO_CS`.
- GPIO character-device ABI v2 owns explicit CS, reset, and HRDY lines.
- The portable transport represents complete transactions, supports streamed
  multiword data, and provides delay support for bounded controller polling.
- Host HRDY waits and portable display-engine polling share one monotonic
  operation deadline.
- The initial physical backend advertised full packed-Gray4 INIT/GC16 and
  refreshed through the explicit buffer-address command. ADR 0007 later added
  canonical Gray8 by streaming a measured Gray8-to-Gray4 conversion. Packed
  formats use MSB-pixel-first bytes, byte-aligned rows, and white unused low
  bits. The backend converts Gray4 to the controller's
  first-pixel-in-low-nibble order.
- A dedicated diagnostic requires an exact named local profile and
  `--allow-hardware`; visible refresh additionally requires `--allow-refresh`.
  Probe never writes VCOM. Session VCOM mutation additionally requires
  `--allow-vcom-write`, pinned firmware/LUT identity, and verified readback.
  Calibration carries that authorization atomically with refresh authorization
  because controller reset may restore a boot VCOM. A previously initialized
  display similarly reprobes identity before reapplying and verifying its
  authorized VCOM on wake. SPI is capped at the 12.5 MHz rate audited in
  Waveshare's Pi implementation. Calibration sleeps during its bounded
  observation interval, then reverifies identity and reapplies/verifies FPC
  VCOM before cleanup. Hangup/termination signals plus scope exit request sleep.
- Deployment executes every shipped binary in a hardware-free mode before
  atomically activating one release directory.

## Consequences

The Linux host requires a device-tree configuration that leaves SPI available
without assigning its native CS pin, and the panel profile now includes
`cs_line`. Protocol behavior remains usable from a future MCU transport. The
first lab path is intentionally full-screen and operator-controlled; partial,
A2, daemon, and unattended recovery work remain later slices.
