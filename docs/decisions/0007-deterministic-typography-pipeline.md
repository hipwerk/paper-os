# ADR 0007: Deterministic typography pipeline

- Status: accepted
- Date: 2026-07-24

## Context

PaperOS had retained scene commands and a shaping/rasterization boundary, but no
complete scene-to-framebuffer path. Host font discovery also made a golden page
dependent on the machine. The development panel is mounted in portrait while
the IT8951 exposes a native 1448×1072 landscape buffer, and the proven
controller upload path is packed Gray4.

The next slice must prove that one application page can produce identical
canonical pixels for desktop review and physical hardware without adding
controller knowledge to the application.

## Decision

- `paper-ui` rasterizes ordered Fill, Stroke, and Text scene commands to a
  canonical Gray8 framebuffer.
- `paper-text` can create an isolated engine from caller-supplied font bytes.
  The reference specimen loads only pinned Source Serif 4 and Noto Naskh Arabic
  assets, whose revision, checksums, and OFL-1.1 licenses are recorded under
  `assets/fonts/`.
- `paper-graphics` owns coverage alpha composition and exact right-angle
  framebuffer rotation.
- Named Linux panel profiles carry a validated clockwise mounting rotation.
- The IT8951 display advertises full-screen Gray8 INIT/GC16 and converts each
  four-pixel group to controller-order Gray4 while streaming. It does not
  allocate a second packed framebuffer.
- `paperos-specimen` owns one deterministic multilingual portrait page. Its
  desktop binary writes a PGM; the guarded hardware diagnostic renders the same
  Gray8 framebuffer, rotates it according to the profile, and uses the existing
  identity/VCOM/refresh/cleanup safety sequence.
- CI compares the rendered Gray8 PGM byte-for-byte with one visually accepted
  golden file.

## Consequences

Application and UI crates remain independent of Linux and IT8951. Preview and
hardware cannot silently diverge before target rotation and pixel conversion.
Font assets add repository and binary size, but remove system-font
nondeterminism and preserve documented redistribution rights.

Gray8 currently means deterministic 16-level quantization on this backend, not
native 8-bit panel output. This decision does not add dithering, partial
refresh, A2, arbitrary-angle transforms, a daemon, or general-purpose widget
features. The physical specimen remains a separately authorized visual test;
its desktop golden proves pixels, not appearance on glass.
