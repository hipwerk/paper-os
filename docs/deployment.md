# Raspberry Pi deployment

The Raspberry Pi is the first Linux host, not part of the application API. Use
64-bit Raspberry Pi OS Lite on a Pi 3/4/5 when possible and the Rust target
`aarch64-unknown-linux-gnu`.

## Development loops

### Native build on the Pi

This is the lowest-friction bring-up path and avoids cross-linker surprises:

```sh
rsync -az --delete --exclude target ./ "$PAPEROS_PI_USER@$PAPEROS_PI_HOST:~/paper-os/"
ssh "$PAPEROS_PI_USER@$PAPEROS_PI_HOST" \
  'cd ~/paper-os && cargo build --release -p daily'
```

The first build is slower; incremental builds are appropriate during driver
bring-up.

### Cross-build from macOS/Linux

Install Zig and cargo-zigbuild, then use the repository script:

```sh
brew install zig                  # macOS example
cargo install --locked cargo-zigbuild
export PAPEROS_PI_HOST=paperos.local
export PAPEROS_PI_USER=<ssh-user>
./scripts/deploy-pi
```

`cargo-zigbuild` supplies the Linux linker/sysroot that `rustup target add`
alone does not provide. The deployment script uploads to a temporary path and
uses `install` for an atomic replacement. It currently installs the simulator
Daily binary for architecture smoke testing; it does not touch a panel.

## Pi host preparation

1. Enable SPI with `raspi-config`, reboot, and verify `/dev/spidev0.0`.
2. Confirm the GPIO chip and line names with `gpioinfo`; never assume a Pi model
   maps chips identically.
3. Create a non-login `paperos` service account and groups permitted to access
   only required devices.
4. Install `deploy/99-paperos.rules` only after reviewing device ownership.
5. Create `/etc/paperos/paperos.env` mode `0640`, owned by root and the service
   group. Copy values from `.env.example`, including the exact panel VCOM.
6. Install the eventual daemon under `/opt/paperos/bin/` and writable state under
   `/var/lib/paperos/`.

Direct peripheral register manipulation is not planned. Linux adapters use
`spidev` and the GPIO character-device v2 API. This works across Pi generations
more cleanly and keeps privileges contained.

## Service design

The production service will:

- run as an unprivileged fixed user with SPI/GPIO group access;
- validate config and probe hardware before any refresh;
- preserve the last good framebuffer and refresh history under
  `/var/lib/paperos`;
- use systemd watchdog notification and bounded operation timeouts;
- stop scheduling updates on repeated hardware faults;
- sleep the controller on graceful shutdown, without blanking the panel;
- log structured update plans and timings to journald.

The unit is intentionally deferred until a long-running hardware binary exists.
A service pointed at the current one-shot preview would create a misleading,
restart-prone deployment.

## Releases and rollback

Build versioned binaries, upload beside the current release, run `--self-test`
without refreshing hardware, then atomically switch a symlink and restart.
Retain the previous binary and config schema for rollback. Never make schema
migration and panel firmware behavior inseparable.

OTA, signed artifacts, and remote management are later milestones. Initial
deployment is SSH-based and operator-controlled.
