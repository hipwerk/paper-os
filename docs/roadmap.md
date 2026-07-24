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

## Slice 1: pixels on glass — complete

- Implemented: Linux `/dev/spidev*` plus GPIO-v2 manual-CS host adapter
- Implemented: ready/reset timing and a shared HRDY/display deadline
- Implemented: named-profile probe reporting size, memory, firmware, LUT, VCOM
- Implemented: streamed packed-Gray4 full upload and explicit-buffer INIT/GC16
  refresh
- Implemented: verified Gray4 boundary packing and diagnostic packing stripe
- Implemented: pinned refresh identity, guarded session VCOM application,
  bounded SPI profile, and address validation
- Implemented: white INIT, sleeping observation, reinitialized VCOM-verified
  cleanup
- Implemented: protocol and host fake-transport tests
- Verified: the unchanged Waveshare demo renders correctly after correcting the
  reversed panel FPC
- Verified: PaperOS's streamed Gray4 upload, explicit-buffer GC16 refresh,
  packing diagnostic, edge border, 16-level ramp, and white cleanup on glass
- Recorded: [controller bring-up, wiring diagnosis, vendor baseline, and
  PaperOS calibration](lab/2026-07-23-waveshare-6in-hd-bring-up.md)

Exit gate: a generated calibration page reaches the physical panel without the
Waveshare C demo, VCOM is verified, and failures time out safely.

The exit gate was met on the named 6-inch fixture with release
`c5fadfbd1b7c8e99`.

## Slice 2: typography and preview — complete

- Implemented: `cosmic-text` integration in the scene-to-Gray8 renderer
- Implemented: pinned OFL-licensed Source Serif 4 and Noto Naskh Arabic assets
  with provenance and checksums
- Implemented: paragraph shaping, bidi, kerning, ligatures, wrapping, and
  bounded overflow through the existing text seam
- Implemented: grayscale glyph composition, scene clipping, and exact
  right-angle framebuffer rotation
- Implemented: deterministic multilingual golden page and desktop PGM preview
- Implemented: named-profile mounting rotation and a guarded hardware specimen
- Verified: the accepted portrait specimen rendered with print-quality text on
  the named physical fixture and completed white cleanup

Exit gate: a multilingual specimen page is pixel-stable in CI and visually
resembles print on the panel.

The exit gate was met on the named 6-inch fixture with release
`1e5c7b7d553b7884`.

## Slice 3: declarative page

- Padding, Align, Row, Column, Stack, Spacer, Overlay
- typed spacing/type/color theme tokens
- nested widget layout through the explicit render context
- Daily page implemented through `paper-ui`, not direct framebuffer calls

Exit gate: Daily renders the same accepted page in simulator and hardware.

## Slice 4: deliberate refresh

- remaining 1/2-bpp packing and measured dithering policies
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
