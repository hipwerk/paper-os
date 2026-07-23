# ADR 0001: Retained scene with canonical Gray8 rasterization

- Status: accepted
- Date: 2026-07-23

## Context

Direct framebuffer rendering is simple on a Raspberry Pi but loses semantic
structure before optimization and assumes enough memory for full current and
previous frames. Future constrained targets need tiling; desktop inspection and
golden testing need deterministic intermediate semantics.

## Decision

Applications produce an ordered retained scene/display list. Layout and text
produce draw commands; a renderer rasterizes to canonical Gray8. Target adapters
pack/dither Gray8 to supported panel encodings.

## Consequences

The scene is an explicit API and consumes allocation on the full host profile.
It enables inspection, tiling, alternative rasterizers, and semantic damage
tracking. Packed controller pixels never contaminate UI/layout APIs.
