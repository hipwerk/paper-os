# ADR 0002: Portable controller core with host adapters

- Status: accepted
- Date: 2026-07-23

## Context

IT8951 commands are portable, but SPI/GPIO setup differs across Linux and MCU
targets. Putting Raspberry Pi libraries in the driver would make the controller
crate host-specific and frustrate later hardware.

## Decision

`paper-it8951` is `no_std` and depends on a small transaction-oriented
`Transport`. Linux adapters will use spidev and GPIO character-device ABI v2;
MCU adapters will use embedded-hal implementations.

## Consequences

Protocol sequences can be tested with a fake transport on every host.
Transaction framing and ready-pin synchronization live in adapters and require
their own tests. Application and rendering crates cannot depend on adapters.
