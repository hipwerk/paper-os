## Outcome

What changes for users or developers?

## Architecture and safety

Which crate boundaries, ADRs, display capabilities, or panel-health concerns
are involved?

## Verification

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo metadata --locked --no-deps --format-version 1`
- [ ] `cargo clippy --locked --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --locked --workspace --all-targets --all-features`
- [ ] `shellcheck scripts/*` when deployment scripts changed
- [ ] Relevant preview/golden output reviewed
- [ ] Pi target checked when portable code changed
- [ ] `thumbv7em-none-eabihf` core checked when portable code changed
- [ ] Hardware result documented when physical behavior is claimed
