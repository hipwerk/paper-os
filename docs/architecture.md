# Architecture

## Dependency direction

```text
apps
  ↓
paper-ui ─────→ paper-layout
  │                  ↓
  ├──────────→ paper-text
  ↓
paper-graphics ─→ paper-display
  ↓                    ↑
paper-runtime ─────────┘
                       ↑
paper-simulator   paper-it8951
                       ↑
                 Linux/MCU adapters (next)
```

Applications and UI never import a controller or host adapter. The IT8951 crate
depends only on controller-agnostic data and exposes a transport seam that SPI
and GPIO implementations satisfy.

## Pipeline

```text
Application state + explicit event
              ↓
         Widget tree + logical scale
              ↓ bounded layout
        retained Scene
              ↓ text shaping + rasterization
     canonical Gray8 surface
              ↓ compare previous surface
      semantic RefreshPlan
              ↓ format packing + validation
       Display backend update
              ↓
       controller and panel
```

Gray8 is canonical because typography and image processing need tonal headroom.
Packing to 1/2/4-bpp and dithering are target transforms; they are not allowed
to leak into application layout.

The retained `Scene` is an ordered display list, not a general mutable view
hierarchy. It provides enough structure for inspection, tiled rasterization,
semantic damage tracking, and deterministic testing without turning PaperOS
into a conventional event-heavy GUI toolkit.

Application design values are logical units. A rational `ScaleFactor` in the
render context resolves them deterministically to physical pixels before
framebuffer drawing. Text shaping receives resolved physical metrics so hinting
and rasterization happen at the target resolution.

## Crate contracts

### paper-display

Owns geometry, pixel encodings, semantic waveform intent, discovered
capabilities, update requests, and the physical `Display` trait. It has no
controller names. A display may reject a request that violates capabilities.

The current alignment model is intentionally minimal and will expand to
mode-specific addressable bounds during the first real upload implementation.

### paper-it8951

Owns protocol commands, device-info parsing, LUT interpretation, VCOM typing,
sleep/wake, and eventually upload/refresh. It is `no_std` and delegates exact
SPI/GPIO transactions to a `Transport`.

It must not own Linux device paths, `spidev`, Raspberry Pi pin numbering, layout,
or refresh policy. VCOM is a required typed input and is never defaulted.

### paper-runtime

Owns previous-frame history, changed pixels/regions, partial-update aging, and
semantic refresh plans. Planning is pure: history advances only after the
backend reports a successful update. Fast monochrome requires binary source and
destination pixels across the final aligned region plus explicit format and
waveform capabilities. The initial planner uses one conservative bounding
rectangle; future planners may emit multiple regions after measurements justify
the complexity.

### paper-graphics

Owns deterministic raster storage and drawing. The initial Gray8 framebuffer is
contiguous and host-friendly. Future tiled surfaces must preserve the same
observable pixel semantics.

### paper-text

Owns font selection, shaping, bidi, wrapping, metrics, truncation, and glyph
rasterization boundaries. The first backend is `cosmic-text` 0.19 with HarfRust
shaping, font discovery, layout, and Swash rasterization. Its public seam emits
coverage rectangles, not glyph IDs, so fallback font identity stays with the
engine that can rasterize it. PaperOS will bundle explicitly licensed fonts for
deterministic production rendering instead of relying on host discovery.

### paper-layout

Owns validated constraints, logical scaling, and resolution-independent
placement. The allocation-free `no_std` linear core distributes rounding
remainders exactly and proportionally shrinks overflow to stay within bounds.
Stack, Padding, Align, Overlay, and Grid follow.

### paper-ui

Owns application-facing composition, widgets, themes, and scenes. A widget
measures within constraints and draws into the final rectangle assigned by its
parent; hidden mutable layout caches are not part of the contract. Bounded text
commands retain content, resolved style, ink, line limit, and overflow policy.
Themes expose semantic tokens and remain explicit layout inputs.

### paper-assets

Owns stable identifiers, licensed asset manifests, decoding/transform policy,
and eventual caches. Decoder libraries remain behind this boundary.

### paper-simulator

Implements a deterministic in-memory display and host artifacts. Preview image
files are permitted as development outputs; the device pipeline never requires
an intermediate image file.

## Portability tiers

- **Core portable:** display contract, IT8951 protocol, geometry, core layout;
  continuously compiled for `thumbv7em-none-eabihf`.
- **Alloc portable:** scenes and richer layout on targets with an allocator.
- **Host/large target:** full text/font database, image decoders, contiguous
  Gray8 frame history, network-backed apps.
- **Constrained target:** selected fonts/assets, tiled rendering, bounded
  caches, and controller/external RAM.

Crates should move toward `no_std` only where an actual target and memory budget
prove the API. `no_std` is not a badge and must not make the host product worse.

## Error and observability model

No physical update is fire-and-forget. Backends will return structured errors
and an update report containing region, format, waveform mapping, upload time,
ready time, and cleanup counters. The daemon will log these as structured
events. Application content errors may preserve the last good frame rather than
blanking the persistent display.
