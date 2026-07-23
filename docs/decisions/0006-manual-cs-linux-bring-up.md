# ADR 0006: Manual-CS Linux IT8951 bring-up

- Status: accepted
- Date: 2026-07-23

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
- The first physical backend advertises full packed-Gray4 INIT/GC16 only and
  refreshes through the explicit buffer-address command. Direct Gray8 remains
  unadvertised until a measured conversion/cost policy exists. Packed formats
  use MSB-pixel-first bytes, byte-aligned rows, and white unused low bits.
- A dedicated diagnostic requires an exact named local profile and
  `--allow-hardware`; visible refresh additionally requires `--allow-refresh`.
  Probe and calibration never write VCOM. Calibration sleeps during its bounded
  observation interval, reinitializes before cleanup, and handles termination
  signals as graceful sleep requests.
- Deployment executes both binaries in hardware-free modes before atomically
  activating one release directory.

## Consequences

The Linux host requires a device-tree configuration that leaves SPI available
without assigning its native CS pin, and the panel profile now includes
`cs_line`. Protocol behavior remains usable from a future MCU transport. The
first lab path is intentionally full-screen and operator-controlled; partial,
A2, daemon, and unattended recovery work remain later slices.
