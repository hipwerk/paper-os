# Roadmap

Milestones are vertical and evidence-based. “Done” means a behavior is tested,
documented, and demonstrated at its lowest real boundary.

## Foundation — complete

- Rust 1.97.1 workspace with Edition 2024 and resolver 3
- Requested platform crates plus simulator and Daily reference app
- Geometry, Gray8 framebuffer, linear layout, scene model
- Bounding-box diff and conservative refresh planner
- Typed VCOM, IT8951 device probe, LUT-specific A2 mapping
- Unit/property tests, CI, dependency policy, agent guidance
- Pi deployment and Waveshare lab plan

## Slice 1: pixels on glass — next

- Linux host adapter using `/dev/spidev*` and GPIO character-device ABI v2
- IT8951 transaction framing with ready/reset timing and timeouts
- Device probe that records panel size, memory address, firmware, and LUT
- Gray8/4-bpp full upload and GC16 refresh
- Explicit white INIT cleanup and sleep
- Simulator and fake-transport parity tests
- Lab report from the named 6-inch HD panel

Exit gate: a generated calibration page reaches the physical panel without the
Waveshare C demo, VCOM is verified, and failures time out safely.

## Slice 2: typography and preview

- `cosmic-text` backend behind `paper-text`
- Bundled OFL-licensed variable/static font fixtures with license manifest
- paragraph shaping, bidi, kerning, ligatures, wrapping, and ellipsis
- grayscale glyph rasterization into `paper-graphics`
- deterministic golden pages in CI
- desktop preview with panel size, rotation, and pixel-density profiles

Exit gate: a multilingual specimen page is pixel-stable in CI and visually
resembles print on the panel.

## Slice 3: declarative page

- Padding, Align, Row, Column, Stack, Spacer, Overlay
- typed spacing/type/color theme tokens
- scene rasterizer and clipping
- Daily page implemented through `paper-ui`, not direct framebuffer calls

Exit gate: Daily renders the same accepted page in simulator and hardware.

## Slice 4: deliberate refresh

- format packing for 1/2/4/8-bpp with endian tests
- mode-specific legal update regions, including the M641 32-pixel rule
- partial GC16 and A2 updates
- update history persisted across daemon restarts
- cleanup policy informed by temperature and measured ghosting
- structured timing and plan telemetry

Exit gate: repeated clock/content updates stay within an accepted ghosting
envelope and automatically clean up.

## Slice 5: appliance runtime

- configuration schema and validation
- scheduler, data-source isolation, stale-data behavior
- systemd service, watchdog, graceful shutdown, last-good-frame persistence
- signed/reproducible release artifact and atomic rollback
- soak and power-loss tests

Exit gate: unattended operation for weeks with failure evidence.

## Later

Reader/EPUB, Journal/Markdown, weather, art, images/dithering, SVG, PDF, input
events, touch, battery, OTA, additional controllers, constrained MCU profiles,
and custom hardware.
