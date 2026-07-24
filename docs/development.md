# Development

## Toolchain

`rust-toolchain.toml` is the repository authority for Rust 1.97.1, components,
and targets. The portable contributor path is rustup. Hipwerk's optional
community asdf Rust plugin wraps rustup and honors `.tool-versions`, preserving
Rust-native components and per-project overrides.

The workspace uses Rust Edition 2024 and Cargo resolver 3. The toolchain is
pinned, and `Cargo.lock` is committed because the workspace contains deployable
applications and reproducible hardware behavior matters.

```sh
rustup show
rustc --version
cargo --version
```

For the Hipwerk asdf workflow:

```sh
asdf plugin add rust https://github.com/code-lever/asdf-rust.git
asdf install
```

## Daily loop

```sh
cargo fmt --all
cargo test --locked --workspace --all-targets --all-features
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo run -p daily -- artifacts/daily.pgm
cargo run -p paperos-specimen -- artifacts/specimen.pgm
```

Fast tests, dependency policy, workflow linting, typo checks, and coverage use
small developer tools. On macOS they can be installed with:

```sh
brew install just cargo-nextest cargo-deny typos-cli actionlint cargo-llvm-cov shellcheck
```

Portable Cargo installs are also available for the Rust-native tools:

```sh
cargo install --locked cargo-nextest cargo-deny typos-cli cargo-llvm-cov
cargo nextest run --locked --workspace --all-features
cargo deny check
```

For Raspberry Pi cross-deployment, install Zig and `cargo-zigbuild`:

```sh
brew install zig                  # macOS example
just bootstrap-deploy
```

`bootstrap-deploy` verifies that Zig is on `PATH` and installs the locked Cargo
subcommand. Native builds on the Pi do not require either tool.

## Dependency policy

- Prefer mature pure-Rust libraries at well-defined seams.
- Avoid reimplementing shaping, font parsing, and image decoding.
- Keep default features narrow and inspect transitive dependencies.
- New runtime dependencies require license compatibility and an explanation in
  the pull request.
- `Cargo.lock` changes are reviewed like code.
- Dependabot updates Cargo and pinned GitHub Actions weekly.

The host text backend is `cosmic-text` 0.19 rather than independently assembling
shaping, fallback, bidi, wrapping, and rasterization. It remains behind
`paper-text`, which exposes coverage pixels instead of backend cache keys, so a
smaller constrained implementation can satisfy the same contract later.
Deterministic production and golden rendering must load explicitly supplied font
bytes. Bundled font revisions, checksums, and licenses live under
`assets/fonts/`; do not silently substitute host fonts.

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
