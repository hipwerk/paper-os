# PaperOS

[![CI](https://github.com/hipwerk/paper-os/actions/workflows/ci.yml/badge.svg)](https://github.com/hipwerk/paper-os/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

PaperOS is a native Rust platform for designing high-quality interfaces for
reflective displays. Applications describe a page; PaperOS lays it out, shapes
and rasterizes it, plans the least harmful display refresh, and sends it through
a hardware backend.

> SwiftUI for E Ink, with the sensibility of designing a printed page.

The repository is at foundation stage. Geometry, grayscale framebuffer drawing,
layout primitives, refresh planning, an IT8951 protocol core, a simulator, and a
reference app compile and are tested. Text shaping and real panel upload are the
next vertical slice; the current `daily` output is intentionally a placeholder.

## Start here

Prerequisites are macOS or Linux, [asdf](https://asdf-vm.com/) 0.16 or newer,
and Git. The repository pins Rust 1.97.1 in both `.tool-versions` and
`rust-toolchain.toml`; asdf selects the installation and rustup supplies Rust
components and cross targets.

```sh
asdf install
rustup show
cargo test --workspace --all-targets --all-features
cargo run -p daily -- artifacts/daily.pgm
```

Open the generated PGM in Preview, ImageMagick, or another image viewer.
Install the optional tools listed in [development](docs/development.md), then
`just ci` runs the complete local quality gate and `just preview` renders the
sample.

## Workspace

```text
apps/daily             Reference application
crates/paper-display   Display capabilities and update contract
crates/paper-it8951    Portable IT8951 protocol core
crates/paper-runtime   Diffing and refresh policy
crates/paper-graphics  Gray8 framebuffer and drawing
crates/paper-text      Typography API and shaping boundary
crates/paper-layout    Row/Column layout primitives
crates/paper-ui        Scene and widget API
crates/paper-assets    Typed asset metadata
crates/paper-simulator In-memory display and host preview export
```

Read [the product definition](docs/product.md), [architecture](docs/architecture.md),
and [roadmap](docs/roadmap.md) before making a structural change. Raspberry Pi
and physical-panel work starts in [deployment](docs/deployment.md) and the
[Waveshare lab guide](docs/hardware/waveshare-6in-hd.md).

## Quality gate

```sh
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace --all-targets --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps
cargo check --workspace --all-targets --target aarch64-unknown-linux-gnu
cargo deny check
typos
actionlint
```

Physical display tests are never part of the default test suite. They require a
named panel profile, its exact VCOM value, and an explicit operator opt-in.

## License

Licensed under either [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your
option.
