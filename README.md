# PaperOS

[![CI](https://github.com/hipwerk/paper-os/actions/workflows/ci.yml/badge.svg)](https://github.com/hipwerk/paper-os/actions/workflows/ci.yml)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE-MIT)

PaperOS is a native Rust platform for designing high-quality interfaces for
reflective displays. Applications describe a page; PaperOS lays it out, shapes
and rasterizes it, plans the least harmful display refresh, and sends it through
a hardware backend.

> SwiftUI for E Ink, with the sensibility of designing a printed page.

The repository is at foundation/bring-up stage. Geometry, grayscale framebuffer
drawing, bounded layout, deterministic bundled-font shaping, retained-scene
rasterization, safe refresh planning, an IT8951 protocol core, a Linux manual-CS
host adapter, and a simulator compile and are tested. The exact Waveshare
fixture has passed both its vendor-demo baseline and PaperOS's full-screen Gray4
calibration with verified cleanup. The deterministic multilingual typography
specimen has also passed physical visual review with print-quality output,
correct portrait rotation, and verified white cleanup. The current `daily`
output remains intentionally a placeholder.

## Start here

Prerequisites are macOS or Linux, Git, and
[rustup](https://rustup.rs/). `rust-toolchain.toml` installs the pinned compiler,
components, and cross targets automatically:

```sh
rustup show
cargo test --locked --workspace --all-targets --all-features
cargo run -p daily -- artifacts/daily.pgm
cargo run -p paperos-specimen -- artifacts/specimen.pgm
cargo run -p paperos-hardware -- self-test
```

Hipwerk developers may instead use [asdf](https://asdf-vm.com/) 0.16 or newer.
Install its Rust plugin once, then the checked-in `.tool-versions` selects the
same rustup-managed toolchain:

```sh
asdf plugin add rust https://github.com/code-lever/asdf-rust.git
asdf install
```

Open the generated PGM in Preview, ImageMagick, or another image viewer.
Install the optional tools listed in [development](docs/development.md), then
`just ci` runs the complete local quality gate. `just preview` renders the
placeholder Daily page and `just specimen` renders the accepted typography
page.

## Workspace

```text
apps/daily             Reference application
apps/specimen          Deterministic multilingual typography specimen
crates/paper-display   Display capabilities and update contract
crates/paper-it8951    Portable IT8951 protocol core
crates/paper-it8951-linux Linux spidev/GPIO-v2 host adapter
crates/paper-runtime   Diffing and refresh policy
crates/paper-graphics  Gray8 framebuffer and drawing
crates/paper-text      Typography API and shaping boundary
crates/paper-layout    Row/Column layout primitives
crates/paper-ui        Scene and widget API
crates/paper-assets    Typed asset metadata
crates/paper-simulator In-memory display and host preview export
apps/paperos-hardware  Explicit probe/calibration diagnostic
```

Read [the product definition](docs/product.md), [architecture](docs/architecture.md),
and [roadmap](docs/roadmap.md) before making a structural change. Raspberry Pi
and physical-panel work starts in [deployment](docs/deployment.md) and the
[Waveshare lab guide](docs/hardware/waveshare-6in-hd.md).

## Quality gate

```sh
cargo fmt --all -- --check
cargo metadata --locked --no-deps --format-version 1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo clippy --locked --workspace --all-targets --all-features --target aarch64-unknown-linux-gnu -- -D warnings
cargo test --locked --workspace --all-targets --all-features
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --all-features --no-deps
cargo check --locked --workspace --all-targets --target aarch64-unknown-linux-gnu
cargo check --locked -p paper-display -p paper-it8951 -p paper-layout --target thumbv7em-none-eabihf
cargo deny check
typos
actionlint
shellcheck scripts/*
cargo llvm-cov --locked --workspace --all-features --summary-only --fail-under-lines 75
```

Physical display tests are never part of the default test suite. They require a
named panel profile, its exact FPC VCOM value, pinned controller identity, and
separate operator opt-ins for hardware access, session-scoped VCOM writes, and
visible refreshes.

## License

Licensed under either [Apache 2.0](LICENSE-APACHE) or [MIT](LICENSE-MIT), at your
option.
