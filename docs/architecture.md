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
             Linux adapter / future MCU adapters
```

Applications and UI never import a controller or host adapter. The IT8951 crate
depends only on controller-agnostic data and exposes a transport seam that SPI
and GPIO implementations satisfy.

## Pipeline

```text
Application state + explicit event
              ↓
    Widget tree + render context
       (scale + text measurement)
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

Application design values are logical units. The widget render context carries
a rational `ScaleFactor` and the target text-measurement engine through nested
layout. Widgets resolve values deterministically before framebuffer drawing and
measure wrapped text using the same backend that will rasterize it. Text shaping
therefore receives physical metrics at the target resolution.

## Crate contracts

### paper-display

Owns geometry, pixel encodings, semantic waveform intent, discovered
capabilities, update requests, and the physical `Display` trait. Capabilities
advertise complete legal update profiles—format, waveform, partial support, and
alignment together—rather than implying a cross-product. It has no controller
names or numeric controller-mode escape hatch. Alignment constraints are
non-zero validated values; malformed backend data cannot silently become
unrestricted. A display rejects requests that do not match a profile.

The current alignment model is intentionally minimal. It will expand to
mode-specific addressable bounds before partial/A2 operations are exposed.

### paper-it8951

Owns protocol commands, device-info parsing, LUT interpretation, VCOM typing,
deep-sleep reinitialization, bounded display-engine polling, and full packed
Gray4 upload/refresh. It is `no_std` and delegates exact SPI/GPIO transactions
to a `Transport`.

It must not own Linux device paths, `spidev`, Raspberry Pi pin numbering, layout,
or refresh policy. VCOM is a required typed input, is never defaulted, and a
write succeeds only after matching controller readback.

The first physical `Display` implementation advertises only full-screen packed
Gray4 INIT/GC16. LUT family, A2 mode, and alignment remain controller metadata,
not executable `DisplayCapabilities`. Direct Gray8 is deliberately not
advertised because the controller cannot bulk-write it; canonical Gray8 will be
converted to a measured packed format at the display boundary. Packed display
bytes are MSB-pixel-first with byte-aligned rows and white low-order padding
bits. Refresh uses the explicit buffer-address command.

### paper-it8951-linux

Owns named panel-profile parsing, Linux spidev setup, GPIO character-device ABI
v2 requests, manual CS, reset timing, and bounded HRDY polling. spidev runs in
mode 0 with `SPI_NO_CS`; GPIO owns CS because the IT8951 requires HRDY
synchronization while CS remains asserted after the transaction preamble.
Display polling establishes one monotonic deadline shared by every nested HRDY
wait. Applications and portable controller code do not depend on this crate.

### paper-runtime

Owns the last committed framebuffer, changed pixels/regions, partial-update
aging, and semantic refresh plans. Planning returns an opaque pending update
bound to one runtime generation. Consuming it after backend success atomically
advances both framebuffer and aging history; an indeterminate backend result
invalidates pending work and forces a full cleanup. Cold starts are uncertain
and also require a full successful update; only explicitly restored,
success-confirmed framebuffer and partial-count state may resume incremental
planning. Fast monochrome requires binary source and destination pixels across
the final aligned region plus an exact advertised update profile. The initial
planner uses one conservative bounding rectangle; future planners may emit
multiple regions after measurements justify the complexity.

### paper-graphics

Owns deterministic raster storage and drawing. The initial Gray8 framebuffer is
contiguous and host-friendly. Future tiled surfaces must preserve the same
observable pixel semantics.

### paper-text

Owns font selection, shaping, bidi, wrapping, metrics, truncation, and glyph
rasterization boundaries. The first backend is `cosmic-text` 0.19 with HarfRust
shaping, font discovery, layout, and Swash rasterization. Its public seam emits
coverage rectangles, not glyph IDs, so fallback font identity stays with the
engine that can rasterize it. Coverage is clipped to the paragraph bounds and
empty bounds emit nothing. PaperOS will bundle explicitly licensed fonts for
deterministic production rendering instead of relying on host discovery.

### paper-layout

Owns validated constraints, logical scaling, and resolution-independent
placement. The allocation-free `no_std` linear core distributes rounding
remainders exactly and proportionally shrinks overflow to stay within bounds.
Stack, Padding, Align, Overlay, and Grid follow.

### paper-ui

Owns application-facing composition, widgets, themes, and scenes. A widget
measures within constraints using an explicit render context and draws into the
final rectangle assigned by its parent; hidden mutable layout caches are not
part of the contract. Bounded text commands retain content, resolved style,
ink, line limit, and overflow policy. Themes expose semantic tokens and remain
explicit layout inputs.

### paper-assets

Owns stable identifiers, licensed asset manifests, decoding/transform policy,
and eventual caches. Decoder libraries remain behind this boundary.

### paper-simulator

Implements a deterministic in-memory Gray8 display and host artifacts. It
advertises only the update profiles it can execute; packed fast-monochrome
simulation arrives with the packing layer. Preview image files are permitted as
development outputs; the device pipeline never requires an intermediate image
file.

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
