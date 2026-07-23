# Testing strategy

## Default, hardware-free gate

Every developer and CI run executes without devices:

1. unit tests for data types and boundary failures;
2. property tests for geometry, packing, clipping, diff, and layout invariants;
3. fake-transport tests for exact controller command sequences;
4. simulator integration tests for complete update requests;
5. deterministic golden render tests once text lands;
6. clippy, rustfmt, rustdoc, dependency advisory/license checks;
7. compile-check for `aarch64-unknown-linux-gnu`.

CI enforces a 75% workspace line-coverage floor. Coverage is a regression
signal, not a substitute for assertions or hardware evidence. Run `just
coverage` for an HTML report.

No ignored test may silently become a physical test. Hardware access will live
in a dedicated binary and require explicit configuration plus opt-in.

## Golden rendering

Golden files should exercise the scene-to-Gray8 result, not platform-native font
APIs or PNG metadata. Bundle test fonts with known licenses and fixed versions.
Store a small number of meaningful page fixtures. A golden update must include a
human-readable reason and visual review.

Include text specimens for Latin, German/French punctuation, RTL, combining
marks, ligatures, line breaking, fallback, very long words, clipping, and
ellipsizing.

## Protocol testing

The IT8951 transport fake records commands, words, reads, reset, and ready
behavior. Add fixtures from logic-analyzer traces once hardware bring-up begins.
Test timeouts, stuck-ready, short transfers, invalid device info, unsupported
LUTs, alignment boundaries, and VCOM mismatch.

## Physical test ladder

Each step requires the local panel profile and operator authorization:

1. **Baseline:** run the vendor C demo and record panel/controller identity.
2. **Probe only:** reset, wake, read info and VCOM, then sleep; no VCOM write.
3. **Safe full update:** white INIT, calibration Gray8/GC16 page, white cleanup.
4. **Partial grayscale:** fixed small regions and edge cases.
5. **Fast monochrome:** legal aligned regions away from unsupported edge pixels.
6. **Stress:** repeated patterns with scheduled cleanup and temperature record.
7. **Soak:** daemon operation, power interruption, network loss, and recovery.

Record upload duration, controller-ready duration, visible transition, region,
format, firmware/LUT, SPI rate, ambient temperature, update count, and an
operator/photo assessment. A single successful call or completed job is not
proof of acceptable panel output.

## Future hardware-in-the-loop CI

Use a dedicated Pi runner with one named fixture, no public-fork execution, a
protected GitHub environment requiring approval, serialized access, and a
physical power relay for recovery. HIL validates protocol and regressions; it
does not replace simulator tests or visual review.
