# Roadmap

Milestones are vertical and evidence-based. “Done” means a behavior is tested,
documented, and demonstrated at its lowest real boundary.

## Foundation — complete

- Rust 1.97.1 workspace with Edition 2024 and resolver 3
- Requested platform crates plus simulator and Daily reference app
- Geometry, Gray8 framebuffer, linear layout, scene model
- Validated constraints, rational logical scaling, exact fit/shrink layout
- Transactional bounding-box diff/runtime with opaque success commit
- `cosmic-text` shaping/rasterization backend boundary
- Scale- and typography-aware widget render context
- Legal format/waveform update profiles and conservative Gray8 simulator
- Bounded text coverage with empty-region handling
- Non-mutating IT8951 probe, verified typed VCOM write, fail-closed LUT mapping
- Unit/property tests, CI, dependency policy, agent guidance
- Pi deployment and Waveshare lab plan

## Slice 1: pixels on glass — implementation complete, lab evidence pending

- Implemented: Linux `/dev/spidev*` plus GPIO-v2 manual-CS host adapter
- Implemented: ready/reset timing and a shared HRDY/display deadline
- Implemented: named-profile probe reporting size, memory, firmware, LUT, VCOM
- Implemented: packed-Gray4 full upload and explicit-buffer INIT/GC16 refresh
- Implemented: verified Gray4 boundary packing and diagnostic packing stripe
- Implemented: pinned refresh identity, bounded SPI profile, address validation
- Implemented: white INIT, sleeping observation, reinitialized guarded cleanup
- Implemented: protocol and host fake-transport tests
- Remaining: lab report from the named 6-inch HD panel

Exit gate: a generated calibration page reaches the physical panel without the
Waveshare C demo, VCOM is verified, and failures time out safely.

The exit gate is not met until the physical result is observed and recorded.

## Slice 2: typography and preview

- integrate the existing `cosmic-text` backend into the scene-to-Gray8 renderer
- Bundled OFL-licensed variable/static font fixtures with license manifest
- paragraph shaping, bidi, kerning, ligatures, wrapping, and ellipsis
- grayscale glyph composition and scene clipping in `paper-graphics`
- deterministic golden pages in CI
- desktop preview with panel size, rotation, and pixel-density profiles

Exit gate: a multilingual specimen page is pixel-stable in CI and visually
resembles print on the panel.

## Slice 3: declarative page

- Padding, Align, Row, Column, Stack, Spacer, Overlay
- typed spacing/type/color theme tokens
- nested widget layout through the explicit render context
- Daily page implemented through `paper-ui`, not direct framebuffer calls

Exit gate: Daily renders the same accepted page in simulator and hardware.

## Slice 4: deliberate refresh

- remaining 1/2-bpp packing and Gray8-to-packed conversion/dithering
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
