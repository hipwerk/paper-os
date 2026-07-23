# Development

## Toolchain

Rust 1.97.1 is installed through the community asdf Rust plugin because this
development environment standardizes language versions with asdf. That plugin
wraps rustup; `rust-toolchain.toml` remains the repository authority for
components and targets. This preserves Rust-native tooling such as components,
target standard libraries, and per-project overrides.

The workspace uses Rust Edition 2024 and Cargo resolver 3. The toolchain is
pinned, and `Cargo.lock` is committed because the workspace contains deployable
applications and reproducible hardware behavior matters.

```sh
asdf install
rustup show
rustc --version
cargo --version
```

## Daily loop

```sh
cargo fmt --all
cargo test --workspace --all-targets --all-features
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo run -p daily -- artifacts/daily.pgm
```

Fast tests, dependency policy, workflow linting, typo checks, and coverage use
small developer tools. On macOS they can be installed with:

```sh
brew install just cargo-nextest cargo-deny typos-cli actionlint cargo-llvm-cov
```

Portable Cargo installs are also available for the Rust-native tools:

```sh
cargo install --locked cargo-nextest cargo-deny typos-cli cargo-llvm-cov
cargo nextest run --workspace --all-features
cargo deny check
```

## Dependency policy

- Prefer mature pure-Rust libraries at well-defined seams.
- Avoid reimplementing shaping, font parsing, and image decoding.
- Keep default features narrow and inspect transitive dependencies.
- New runtime dependencies require license compatibility and an explanation in
  the pull request.
- `Cargo.lock` changes are reviewed like code.
- Dependabot updates Cargo and pinned GitHub Actions weekly.

Current text direction is `cosmic-text` rather than assembling shaping,
fallback, bidi, wrapping, and rasterization independently. The backend remains
behind `paper-text` so PaperOS owns its stable typography contract and can use a
smaller constrained implementation later.

## Adding a crate

A crate needs one clear owner contract, a direction in `architecture.md`, an
inherited workspace package/toolchain configuration, `[lints] workspace = true`,
and tests. Do not create a layer merely to rename another crate's API.

## Documentation and coding agents

The root `AGENTS.md` contains concise commands, constraints, and definition of
done; deeper durable context lives under `docs/`. Nested `AGENTS.md` files should
be added only when a subtree develops genuinely different commands or safety
rules. Record repeated agent mistakes as concrete repository guidance, not
general advice.
