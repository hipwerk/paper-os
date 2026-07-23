default:
    @just --list

bootstrap:
    rustup show
    rustup component add clippy llvm-tools-preview rustfmt rust-src
    rustup target add aarch64-unknown-linux-gnu thumbv7em-none-eabihf

lock:
    cargo metadata --locked --no-deps --format-version 1 > /dev/null

fmt:
    cargo fmt --all -- --check

lint:
    cargo clippy --locked --workspace --all-targets --all-features -- -D warnings

test:
    cargo nextest run --locked --workspace --all-targets --all-features --profile ci
    cargo test --locked --doc --workspace --all-features

test-fast:
    cargo nextest run --locked --workspace --all-features

docs:
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --all-features --no-deps

deny:
    cargo deny check

typos:
    typos

actions:
    actionlint

shell:
    shellcheck scripts/*

pi-check:
    cargo check --locked --workspace --all-targets --target aarch64-unknown-linux-gnu

core-check:
    cargo check --locked -p paper-display -p paper-it8951 -p paper-layout --target thumbv7em-none-eabihf

coverage:
    cargo llvm-cov --locked --workspace --all-features --html

coverage-check:
    cargo llvm-cov --locked --workspace --all-features --summary-only --fail-under-lines 75
    cargo llvm-cov --locked -p paper-layout --all-features --summary-only --fail-under-lines 75
    cargo llvm-cov --locked -p paper-runtime --all-features --summary-only --fail-under-lines 75
    cargo llvm-cov --locked -p paper-simulator --all-features --summary-only --fail-under-lines 75

ci: lock fmt lint test docs deny typos actions shell pi-check core-check coverage-check

preview output="artifacts/daily.pgm":
    cargo run -p daily -- {{ output }}

deploy:
    ./scripts/deploy-pi
