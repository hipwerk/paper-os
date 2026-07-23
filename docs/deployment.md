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
  'cd ~/paper-os && cargo build --release -p daily -p paperos-hardware'
```

The first build is slower; incremental builds are appropriate during driver
bring-up.

### Cross-build from macOS/Linux

Install Zig and cargo-zigbuild, then use the repository script:

```sh
brew install zig                  # macOS example
just bootstrap-deploy
export PAPEROS_PI_HOST=paperos.local
export PAPEROS_PI_USER=<ssh-user>
./scripts/deploy-pi
```

`cargo-zigbuild` supplies the Linux linker/sysroot that `rustup target add`
alone does not provide. The deployment script builds `daily` and
`paperos-hardware`, copies them to remote `mktemp` paths, executes the hardware
diagnostic's device-free `self-test`, runs Daily to a temporary PGM, and checks
that artifact before installation. It installs both under one
content-addressed `/opt/paperos/releases/<id>/` directory and atomically moves
`/opt/paperos/current`. Deployment and smoke testing do not open SPI/GPIO or
touch the panel.

## Pi host preparation

1. Enable SPI with `raspi-config`.
2. Configure SPI without native chip select by adding
   `dtoverlay=spi0-0cs` to `/boot/firmware/config.txt` on current Raspberry Pi
   OS (or the boot config path reported by that OS), then reboot. PaperOS uses
   `SPI_NO_CS` and drives BCM 8 through GPIO so CS can remain asserted while
   HRDY is sampled. Verify `/dev/spidev0.0` after reboot.
3. Confirm the GPIO chip and line names with `gpioinfo`; never assume a Pi model
   maps chips identically.
4. Create a non-login `paperos` service account and groups permitted to access
   only required devices.
5. Install `deploy/99-paperos.rules` only after reviewing device ownership.
6. Keep physical panel settings, including exact VCOM, in the named
   `hardware/panels.local.toml` profile. It is the sole diagnostic hardware
   configuration authority. Keep SPI at 1 MHz for initial bring-up; validation
   caps it at the audited Waveshare rate of 12.5 MHz.
7. Install the eventual daemon under `/opt/paperos/bin/` and writable state under
   `/var/lib/paperos/`.

Direct peripheral register manipulation is not planned. Linux adapters use
`spidev` and the GPIO character-device v2 API. This works across Pi generations
more cleanly and keeps privileges contained.

## Explicit hardware diagnostics

The deploy script never invokes these commands. From the repository on the Pi,
copy and complete `hardware/panels.local.toml`, then run the ladder only with
operator authorization:

```sh
cargo run --release -p paperos-hardware -- \
  probe --config hardware/panels.local.toml --profile desk-6in-hd \
  --allow-hardware

cargo run --release -p paperos-hardware -- \
  calibrate --config hardware/panels.local.toml --profile desk-6in-hd \
  --allow-hardware --allow-refresh
```

`probe` resets, wakes, reads identity and VCOM, verifies them against the named
profile, and sleeps. The first probe may omit `expected_firmware` and
`expected_lut`; copy its exact output into those local fields and repeat probe
before calibration. `calibrate` refuses unpinned identity, then performs white
INIT, a packed Gray4 GC16 test page with an order-sensitive packing stripe,
sleeps the controller during the bounded observation interval,
resets/reprobes/revalidates identity and VCOM, performs white INIT cleanup, and
sleeps again. `SIGHUP`/`SIGINT`/`SIGTERM` request graceful sleep, and an armed
scope guard attempts sleep on unexpected early return. An interruption while
already sleeping leaves the diagnostic page visible. Neither command writes
VCOM. Any profile/probe mismatch aborts before a refresh.

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

Build versioned binaries, upload beside the current release, run `self-test`
without refreshing hardware, then atomically switch a symlink and restart.
Retain the previous binary and config schema for rollback. Never make schema
migration and panel firmware behavior inseparable.

OTA, signed artifacts, and remote management are later milestones. Initial
deployment is SSH-based and operator-controlled.
