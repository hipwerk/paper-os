# ADR 0005: Explicit render environments and update transactions

- Status: accepted
- Date: 2026-07-23

## Context

The first stabilized APIs still left two critical relationships to caller
discipline. Widgets could not access scale or text measurement during nested
layout, and refresh success accounting was separate from previous-frame
history. Display formats and waveforms were also advertised independently even
though controllers support specific combinations.

These seams would make typography-dependent layout inaccurate and allow
software history to diverge from the persistent panel.

## Decision

- The UI context carries viewport, logical scale, and an object-safe text engine
  through application rendering and widget layout.
- Text engines use one backend-neutral error contract and clip emitted coverage
  to paragraph bounds.
- The refresh runtime owns the last committed framebuffer. Planning returns an
  opaque generation-bound pending update; success consumes it and atomically
  commits framebuffer plus panel-aging history.
- A cold-start runtime begins with uncertain panel state and forces a full
  update. Incremental planning after restart requires explicitly restored,
  success-confirmed framebuffer and partial-refresh history.
- An indeterminate backend outcome invalidates pending updates and forces the
  next plan to be a full quality refresh.
- Display capabilities contain complete update profiles pairing format,
  semantic waveform, partial support, and validated non-zero alignment. Invalid
  alignment cannot silently degrade to unrestricted operation. Numeric
  controller-specific waveform escape values are not part of `paper-display`.
- IT8951 VCOM mutation succeeds only after matching readback.

## Consequences

The unreleased widget, text-engine, refresh, and display-capability APIs break.
Reusable widgets can now measure with their target environment, stale refresh
commits are rejected, and unsupported format/waveform combinations cannot be
inferred accidentally. Fresh processes cannot trust a guessed persistent-panel
state or reset cleanup history implicitly. The current simulator remains
honestly Gray8-only until packed-format support is implemented.
