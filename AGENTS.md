# PaperOS repository guidance

PaperOS is a native Rust rendering and runtime platform for E Ink. It is not an
OS, Linux distribution, web renderer, or Waveshare-only application. Preserve
application portability and treat typography, deterministic output, and panel
health as product requirements.

Hardware status: the Linux IT8951 path has physically passed probe, VCOM
verification, full packed-Gray4 calibration, and white cleanup on the named
Waveshare fixture. Full-screen canonical Gray8 is deterministically streamed as
packed Gray4 at the backend boundary. The deterministic multilingual typography
specimen has matching desktop golden evidence and a successful physical
print-quality review with white cleanup. Partial and fast refresh remain
unavailable.

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
- Pi lint: `cargo clippy --locked --workspace --all-targets --all-features --target aarch64-unknown-linux-gnu -- -D warnings`
- Test: `cargo test --locked --workspace --all-targets --all-features`
- Docs: `RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --all-features --no-deps`
- Pi target: `cargo check --locked --workspace --all-targets --target aarch64-unknown-linux-gnu`
- Portable core: `cargo check --locked -p paper-display -p paper-it8951 -p paper-layout --target thumbv7em-none-eabihf`
- Preview: `cargo run -p daily -- artifacts/daily.pgm`
- Typography specimen: `cargo run -p paperos-specimen -- artifacts/specimen.pgm`
- Hardware-free diagnostic: `cargo run -p paperos-hardware -- self-test`
- Full local gate: `just ci` when `just` is installed.

## Protected `main` workflow

`main` rejects direct pushes. It requires a pull request, the
`Format, lint, test, and docs` status check, and a squash merge. Merge commits
and rebase merges are disabled. When a user asks to “commit and push to main,”
complete this entire workflow instead of trying `git push origin main`.

1. Start from an up-to-date `main` and create a short-lived branch before
   committing:

   ```sh
   git fetch origin
   git switch main
   git pull --ff-only origin main
   git switch -c codex/<short-topic>
   ```

   If work is already uncommitted on local `main`, create the branch immediately
   with `git switch -c codex/<short-topic>`; the working tree carries across.
   Do not create the commit on local `main`.
2. Run `just ci`, stage deliberately, verify `git diff --cached --check`, and
   commit on the topic branch.
3. Push the branch and open a PR targeting `main`:

   ```sh
   git push -u origin HEAD
   gh pr create --base main --head codex/<short-topic>
   gh pr checks <pr-number> --watch
   ```

4. Fix any CI failure on the same branch and wait for the required check to
   pass. Then use the repository's only allowed merge method:

   ```sh
   gh pr merge <pr-number> --squash --delete-branch
   ```

5. Synchronize and clean up locally:

   ```sh
   git switch main
   git pull --ff-only origin main
   git branch -D codex/<short-topic>
   git status --short --branch
   test "$(git rev-parse HEAD)" = "$(git rev-parse origin/main)"
   ```

   Delete the local topic branch only after GitHub reports the PR as merged.
   The squash commit on `main` is the canonical commit to report; its hash
   differs from the topic-branch commit. The final worktree must be clean and
   local `main` must equal `origin/main`.

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
- Linux IT8951 transactions use spidev with hardware CS disabled and a
  GPIO-v2-controlled CS line. Keep CS asserted across preamble/HRDY/payload and
  keep every HRDY/display wait under a shared monotonic deadline.
- IT8951 deep sleep requires reset, reprobe, identity verification, and VCOM
  verification before another update.
- Keep PaperOS packed pixels MSB-first. IT8951 Gray4 nibble reversal belongs
  only in its display adapter. A physical refresh requires pinned firmware/LUT
  identity and an SPI profile no faster than the audited 12.5 MHz ceiling.
- Do not commit fonts, photos, books, API keys, `.env` files, panel-local
  profiles, or other assets unless their license and provenance are documented.

## Definition of done

Relevant tests exist and pass; formatting, clippy, and docs pass; the Pi target
still checks when portable code changes; docs/ADR are updated when contracts
change. Hardware-affecting changes include simulator/protocol tests. Include a
lab result only when physical behavior is explicitly authorized and claimed;
otherwise state that hardware was not exercised.
