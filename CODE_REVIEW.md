# Code review

Review for correctness first, then architecture, panel safety, tests, and
maintainability.

- Does dependency direction still match `docs/architecture.md`?
- Are controller quirks represented as capabilities rather than leaked upward?
- Do capabilities advertise legal format/waveform operation profiles rather
  than independent lists?
- Can nested widgets access the same scale and text-measurement environment?
- Does unknown firmware fail closed instead of inheriting a guessed fast mode?
- Can geometry arithmetic overflow, underflow, clip changed pixels, or violate
  packed-row alignment?
- Does a fast refresh inspect old and new pixels across the final aligned region?
- Does consuming one opaque pending update atomically commit frame and aging
  history only after backend success?
- Does a cold start remain uncertain until a full update succeeds, and does a
  restored state include its partial-update history?
- Does an indeterminate backend result force a full recovery update?
- Can malformed alignment data weaken an update profile instead of failing?
- Can a default command touch hardware or change VCOM?
- Does Linux SPI disable hardware CS and keep manual CS asserted while HRDY is
  sampled after the preamble?
- Are HRDY and display-engine waits bounded, and is CS released on every
  transaction failure?
- Does a deep-sleep wake reset, reprobe, and revalidate identity plus VCOM?
- Can interruption leave the controller awake, or can an operator-provided hold
  duration keep it powered indefinitely?
- Does a framebuffer upload use a bulk transaction where the controller permits
  it instead of repeating one preamble per packed word?
- Does an explicit VCOM write verify controller readback?
- Are renders deterministic across machines and are licensed fonts explicit?
- Do tests cover empty, edge-aligned, panel-edge, grayscale, and failure cases?
- Are comments explaining why rather than restating code?
- Are docs and ADRs consistent with the implementation?

Treat an assertion that a hardware refresh “worked” as incomplete without
controller evidence and, where visual quality matters, an observed panel result.
