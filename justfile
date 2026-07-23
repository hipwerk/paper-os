set dotenv-load

default:
    @just --list

bootstrap:
    rustup show
    rustup component add clippy llvm-tools-preview rustfmt rust-src
    rustup target add aarch64-unknown-linux-gnu thumbv7em-none-eabihf

fmt:
    cargo fmt --all -- --check

lint:
    cargo lint

test:
    cargo nextest run --workspace --all-targets --all-features --profile ci
    cargo test --doc --workspace --all-features

test-fast:
    cargo nextest run --workspace --all-features

docs:
    RUSTDOCFLAGS="-D warnings" cargo docs

deny:
    cargo deny check

typos:
    typos

actions:
    actionlint

shell:
    shellcheck scripts/*

pi-check:
    cargo check --workspace --all-targets --target aarch64-unknown-linux-gnu

core-check:
    cargo check -p paper-display -p paper-it8951 -p paper-layout --target thumbv7em-none-eabihf

coverage:
    cargo llvm-cov --workspace --all-features --html

coverage-check:
    cargo llvm-cov --workspace --all-features --summary-only --fail-under-lines 75
    cargo llvm-cov -p paper-layout --all-features --summary-only --fail-under-lines 75
    cargo llvm-cov -p paper-runtime --all-features --summary-only --fail-under-lines 75
    cargo llvm-cov -p paper-simulator --all-features --summary-only --fail-under-lines 75

ci: fmt lint test docs deny typos actions shell pi-check core-check coverage-check

preview output="artifacts/daily.pgm":
    cargo run -p daily -- {{ output }}

deploy:
    ./scripts/deploy-pi
