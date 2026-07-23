# Code review

Review for correctness first, then architecture, panel safety, tests, and
maintainability.

- Does dependency direction still match `docs/architecture.md`?
- Are controller quirks represented as capabilities rather than leaked upward?
- Can geometry arithmetic overflow, underflow, clip changed pixels, or violate
  packed-row alignment?
- Can a default command touch hardware or change VCOM?
- Are renders deterministic across machines and are licensed fonts explicit?
- Do tests cover empty, edge-aligned, panel-edge, grayscale, and failure cases?
- Are comments explaining why rather than restating code?
- Are docs and ADRs consistent with the implementation?

Treat an assertion that a hardware refresh “worked” as incomplete without
controller evidence and, where visual quality matters, an observed panel result.
