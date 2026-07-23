# PaperOS repository guidance

PaperOS is a native Rust rendering and runtime platform for E Ink. It is not an
OS, Linux distribution, web renderer, or Waveshare-only application. Preserve
application portability and treat typography, deterministic output, and panel
health as product requirements.

## Read before changing

- Product intent and non-goals: `docs/product.md`
- Dependency direction and API boundaries: `docs/architecture.md`
- Current and deferred work: `docs/roadmap.md`
- Hardware safety: `docs/hardware/waveshare-6in-hd.md`
- Larger changes: use the template in `PLANS.md` and add an ADR under
  `docs/decisions/` for durable architectural decisions.

## Commands

- Format: `cargo fmt --all -- --check`
- Lint: `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- Test: `cargo test --workspace --all-targets --all-features`
- Docs: `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps`
- Pi target: `cargo check --workspace --all-targets --target aarch64-unknown-linux-gnu`
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
- Never run physical-panel tests, set VCOM, or refresh hardware without explicit
  user authorization and the matching local panel profile. Never guess VCOM.
- Do not commit fonts, photos, books, API keys, `.env` files, panel-local
  profiles, or other assets unless their license and provenance are documented.

## Definition of done

Relevant tests exist and pass; formatting, clippy, and docs pass; the Pi target
still checks when portable code changes; docs/ADR are updated when contracts
change; hardware-affecting changes include simulator/protocol tests and a
documented lab verification result.
