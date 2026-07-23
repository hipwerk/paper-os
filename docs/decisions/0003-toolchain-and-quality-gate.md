# ADR 0003: Pinned stable Rust and reproducible quality gate

- Status: accepted
- Date: 2026-07-23

## Context

The local environment standardizes languages through asdf, while Rust's official
component and target manager is rustup. Hardware behavior and golden output
benefit from a pinned compiler and lockfile.

## Decision

Pin Rust 1.97.1 in `.tool-versions` and `rust-toolchain.toml`. The asdf Rust
plugin selects the installation; its rustup supplies rustfmt, clippy, rust-src,
and the Pi target. Use Edition 2024, resolver 3, workspace lints, a committed
lockfile, CI tests/docs/cross-checks, and cargo-deny.

## Consequences

Updates are explicit pull requests. Contributors who do not use asdf can still
use `rust-toolchain.toml` directly with rustup.
