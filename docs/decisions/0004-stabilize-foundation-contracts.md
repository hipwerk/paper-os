# ADR 0004: Stabilize layout, text, refresh, and controller contracts

- Status: accepted; render-context, refresh-commit, and capability details
  refined by ADR 0005
- Date: 2026-07-23

## Context

The initial scaffold established crate boundaries but exposed placeholder public
APIs. Widgets lost their assigned bounds between layout and drawing, text glyph
IDs lacked the font identity needed for fallback rasterization, refresh planning
changed ghosting history before hardware success, and unknown IT8951 LUTs
optimistically advertised A2.

These contracts would make later vertical slices build on ambiguous or
panel-unsafe behavior.

## Decision

- Application design units resolve through a deterministic rational scale before
  physical layout. Constraints are validated, overflow shrinks proportionally,
  and rounding remainders are fully assigned.
- Widgets draw into the final rectangle assigned by their parent.
- `paper-text` owns shaping and rasterization. Its public engine seam emits
  coverage rectangles and keeps backend glyph/font cache identity internal. The
  first host backend is `cosmic-text` 0.19 with Swash.
- Refresh planning is pure. A caller records success only after the backend
  completes the update.
- Fast monochrome requires advertised waveform/format support and binary old and
  new pixels across the final legal aligned region. Refresh thresholds use
  refreshed area rather than only changed-pixel count.
- IT8951 fast modes are allowlisted by exact known LUT family. Probing never
  writes VCOM. A refresh session requires separate authorization to apply and
  verify the exact FPC VCOM after reset. Controller boot values are observed
  and logged, never inferred as panel targets.

## Consequences

The unreleased placeholder APIs break, which is intentional. The portable
display, layout, and IT8951 core now compiles for `thumbv7em-none-eabihf`.
Host text builds carry the cosmic-text dependency and system-font discovery
until licensed production fonts are bundled. Unknown controller firmware retains
quality grayscale behavior but cannot use fast refresh without new evidence and
tests.
