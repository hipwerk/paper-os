# PaperOS repository guidance

PaperOS is a native Rust rendering and runtime platform for E Ink. It is not an
OS, Linux distribution, web renderer, or Waveshare-only application. Preserve
application portability and treat typography, deterministic output, and panel
health as product requirements.

Hardware status: simulator and portable IT8951 control protocol only. No Linux
display adapter or physical upload path exists yet.

## Read only what the task needs

- Cross-crate or public API work: `docs/architecture.md`
- Product behavior or scope: `docs/product.md` and `docs/roadmap.md`
- Hardware, controller, refresh, or deployment work:
  `docs/hardware/waveshare-6in-hd.md`
- Code review: `CODE_REVIEW.md`
- Multi-crate, backend, or hardware experiments: use `PLANS.md`; record durable
  decisions under `docs/decisions/`.

For a localized change, start with this file and nearby code. Do not load every
linked document by default.

## Commands

- Format: `cargo fmt --all -- --check`
- Lint: `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
- Test: `cargo test --locked --workspace --all-targets --all-features`
- Docs: `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --all-features --no-deps`
- Pi target: `cargo check --locked --workspace --all-targets --target aarch64-unknown-linux-gnu`
- Portable core: `cargo check --locked -p paper-display -p paper-it8951 -p paper-layout --target thumbv7em-none-eabihf`
- Preview: `cargo run -p daily -- artifacts/daily.pgm`
- Full local gate: `just ci` when `just` is installed.

## Engineering rules

- Keep dependencies flowing downward as documented; never let applications or
  rendering crates depend on a controller backend.
- Keep controller protocols portable. Linux SPI/GPIO belongs in a host adapter,
  not `paper-it8951`.
- Render canonically in Gray8. Pack/dither for a panel at the display boundary.
- Prefer semantic capabilities and waveform intent over controller constants in
  public APIs.
- No unreviewed `unsafe`. Workspace lints forbid it.
- Add deterministic tests with every behavior change. Prefer property tests for
  geometry/diff invariants and golden render tests for typography.
- Refresh planning must be pure. Commit an opaque pending update only after a
  backend confirms success; mark indeterminate outcomes uncertain so the next
  plan is full. Cold starts are uncertain; only restore a known panel frame
  together with its committed partial-update count. Unknown controller firmware
  fails closed.
- Never run physical-panel tests, set VCOM, or refresh hardware without explicit
  user authorization and the matching local panel profile. Never guess VCOM.
- Do not commit fonts, photos, books, API keys, `.env` files, panel-local
  profiles, or other assets unless their license and provenance are documented.

## Definition of done

Relevant tests exist and pass; formatting, clippy, and docs pass; the Pi target
still checks when portable code changes; docs/ADR are updated when contracts
change. Hardware-affecting changes include simulator/protocol tests. Include a
lab result only when physical behavior is explicitly authorized and claimed;
otherwise state that hardware was not exercised.
