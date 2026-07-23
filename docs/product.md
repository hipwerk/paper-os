# Product definition

## Vision

PaperOS is a native Rust platform for building beautiful applications on
reflective displays. It is not a dashboard, Raspberry Pi project, browser
renderer, Linux distribution, or controller-specific SDK. The platform—not any
one app—is the product.

Applications should describe a printed-page-like composition without knowing
about SPI, GPIO, IT8951, refresh modes, or packed pixels:

```rust,ignore
Column::new()
    .padding(48)
    .push(Text::new("Today").weight(FontWeight::Bold).size(72))
    .push(CalendarWidget::today())
    .push(WeatherWidget::current())
```

The same application source should target desktop preview, Raspberry Pi E Ink,
and future hardware backends where the target has sufficient capabilities.

## Principles

- **Typography first:** shaping, spacing, hierarchy, and tonal quality matter
  more than animation.
- **Static first:** persistence and deliberate refresh are features.
- **Native:** render directly; no browser, HTML, screenshot, or Electron path.
- **Hardware-independent applications:** hardware differences are capabilities
  and backend policy, not app conditionals.
- **Explicit updates:** each physical refresh has a reason and observable plan.
- **Deterministic:** the same scene, assets, and configuration produce the same
  pixels.
- **Understandable boundaries:** modules may be numerous, but each owns a real
  contract and has tests at that seam.

## Critical corrections to the original brief

### “Runs unchanged” means source portability, not identical execution

A Raspberry Pi can retain a 1.48 MiB Gray8 frame for the 1448×1072 panel. Many
MCUs cannot, and the Waveshare guidance itself uses external SDRAM for STM32.
MCU support will require tiled rendering, controller RAM, reduced features, or
external memory. Application APIs should remain portable, while build features
and supported capabilities may differ.

### A scene/display list is necessary

Rendering applications directly into one framebuffer makes the current Pi easy
but blocks tiling, semantic diffing, inspection, and alternative rasterizers.
Applications therefore produce a retained scene/display list. The renderer may
still rasterize it immediately on capable hosts.

### Refresh policy cannot be universal

“Partial,” “full,” and “grayscale” do not mean the same thing on every panel.
Temperature, waveform/LUT firmware, pixel encoding, alignment, changed content,
and update history matter. The runtime chooses semantic intent; each backend
maps it to supported controller operations and may reject an illegal plan.

### Display capabilities are richer than width and height

Capabilities advertise legal operation profiles that pair pixel format,
waveform, partial-update support, and alignment instead of independent lists.
They will also include addressable regions, power behavior, and eventually
temperature and color planes. A backend discovered at runtime is the authority;
unknown firmware fails closed rather than inheriting a guessed fast mode.

### Interactivity should be admitted without dominating v1

Reader page turns and timers already imply input and time-driven state. The
composition model remains static-first, but the runtime should eventually
accept discrete events and rerender explicitly. Continuous animation and
60-fps assumptions remain non-goals.

### Performance goals need measurement definitions

“Partial refresh under 500 ms” depends on region, mode, SPI clock, temperature,
firmware, and whether upload time is included. Benchmarks will report upload,
controller-ready, visible-transition, region, waveform, temperature, and panel
profile separately.

## Initial product slice

The first convincing slice is not a collection of placeholder widgets. It is:

1. a desktop preview and deterministic golden render;
2. shaped, wrapped, anti-aliased text using the cosmic-text backend and a bundled
   licensed font;
3. one declarative page laid out through `paper-ui`;
4. a diff and legal refresh plan;
5. the same pixels on the Waveshare 6-inch HD panel;
6. measured full, grayscale-partial, and fast-monochrome behavior.

## Non-goals

- General-purpose interactive GUI toolkit
- OS or Linux distribution
- Browser compatibility
- Transparent promise that every app fits every MCU
- Hiding display physics behind an infallible abstraction

## Success

- A new app can produce a polished preview within one hour.
- Application code imports no controller or Linux host crate.
- A backend change requires no application-source change when capabilities
  overlap.
- Typography survives pixel-level golden comparison and physical visual review.
- Refresh decisions are logged, reproducible, and constrained by panel safety.
- Weeks-long Pi operation is demonstrated with recoverable failures.
