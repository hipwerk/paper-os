set dotenv-load

default:
    @just --list

bootstrap:
    rustup show
    rustup component add clippy llvm-tools-preview rustfmt rust-src
    rustup target add aarch64-unknown-linux-gnu

fmt:
    cargo fmt --all -- --check

lint:
    cargo lint

test:
    cargo test --workspace --all-targets --all-features

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

pi-check:
    cargo check --workspace --all-targets --target aarch64-unknown-linux-gnu

coverage:
    cargo llvm-cov --workspace --all-features --html

ci: fmt lint test docs deny typos actions pi-check

preview output="artifacts/daily.pgm":
    cargo run -p daily -- {{ output }}

deploy:
    ./scripts/deploy-pi
