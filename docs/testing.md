# Testing strategy

## Default, hardware-free gate

The full developer and CI gate executes without devices:

1. unit tests for data types and boundary failures;
2. property tests for geometry, clipping, diff, and layout invariants;
3. fake-transport tests for exact controller command sequences;
4. simulator integration tests for the Gray8 update profiles it implements;
5. deterministic golden render tests using the isolated bundled-font database;
6. clippy, rustfmt, rustdoc, dependency advisory/license checks;
7. locked dependency resolution plus compile checks for
   `aarch64-unknown-linux-gnu` and the portable embedded core.

CI enforces a 75% workspace line-coverage floor. Coverage is a regression
signal, not a substitute for assertions or hardware evidence. Layout, runtime,
simulator, portable IT8951 protocol, and Linux IT8951 host adapter also enforce
independent 75% floors so aggregate coverage cannot hide a weak risk-critical
crate. Run `just coverage` for an HTML report.

Deterministic CI tests are not retried. A transient failure is evidence to
investigate rather than a passing result.

No ignored test may silently become a physical test. Hardware access will live
in a dedicated binary and require explicit configuration plus opt-in.

## Golden rendering

Golden files exercise the scene-to-Gray8 result, not platform-native font APIs
or PNG metadata. The accepted typography page is
`apps/specimen/tests/golden/typography-specimen.pgm`; it loads only the pinned
font bytes documented under `assets/fonts/`. Regenerate a candidate with
`just specimen`, compare it visually and byte-for-byte, and replace the golden
only with a human-readable reason.

Include text specimens for Latin, German/French punctuation, RTL, combining
marks, ligatures, line breaking, fallback, very long words, clipping, and
ellipsizing.

## Protocol testing

The IT8951 protocol fake records commands, single and bulk words, reads, delays,
and reset. It verifies probe non-mutation, literal firmware/LUT classification,
VCOM readback, mismatch failure, streamed packed Gray4 upload, explicit-buffer
refresh, PaperOS-to-controller Gray4 nibble conversion, Gray8-to-Gray4
quantization vectors, plausible image-buffer addresses, deep-sleep identity
reprobe and VCOM reapplication, and display deadline. The Linux host fake
verifies preamble byte order, dummy reads,
manual-CS lifetime across HRDY synchronization, bulk transfer, SPI/profile
safety bounds, pinned identity, shared-deadline expiry, dual
transaction/CS-release failure reporting, and stuck HRDY timeout. Short
system calls are handled by
`write_all`/`read_exact`; logic-analyzer evidence belongs in the physical lab
report.

Refresh-runtime tests cover both previous and next pixels across the final
aligned region, sparse damage with large bounds, unsupported profiles,
generation-stale pending updates, cold and restored panel state, uncertain
backend outcomes, and atomic framebuffer plus aging-history commit. Display
contract tests reject zero alignment rather than treating malformed constraints
as unrestricted.

## Physical test ladder

Each step requires the local panel profile and operator authorization:

1. **Baseline:** with all cables locked before power, run the unchanged vendor C
   demo using the exact FPC VCOM and record visible panel behavior plus
   controller identity.
2. **Probe only:** reset, wake, record identity and boot VCOM, then sleep; no
   VCOM write.
3. **Session VCOM:** with pinned identity and explicit authorization, apply the
   FPC value and verify readback; assume reset may restore the boot value.
4. **Safe full update:** atomically verify identity, apply/verify FPC VCOM, then
   run white INIT, packed Gray4/GC16 calibration, and white cleanup.
5. **Typography specimen:** render the accepted canonical Gray8 page, rotate it
   from logical portrait to the panel's native buffer, stream it through the
   Gray4 conversion, assess it visually, and verify white cleanup.
6. **Partial grayscale:** fixed small regions and edge cases.
7. **Fast monochrome:** legal aligned regions away from unsupported edge pixels.
8. **Stress:** repeated patterns with scheduled cleanup and temperature record.
9. **Soak:** daemon operation, power interruption, network loss, and recovery.

Record upload duration, controller-ready duration, visible transition, region,
format, firmware/LUT, SPI rate, ambient temperature, update count, and an
operator/photo assessment. A single successful call or completed job is not
proof of acceptable panel output.

## Future hardware-in-the-loop CI

Use a dedicated Pi runner with one named fixture, no public-fork execution, a
protected GitHub environment requiring approval, serialized access, and a
physical power relay for recovery. HIL validates protocol and regressions; it
does not replace simulator tests or visual review.
