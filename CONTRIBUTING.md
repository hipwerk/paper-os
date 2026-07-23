# Contributing

Start with `README.md` and `AGENTS.md`. Discuss public API, dependency direction,
framebuffer representation, refresh semantics, or hardware safety changes in an
ADR before spreading them across crates.

Every change should be small enough to verify and should include tests at the
lowest useful layer. Run the quality gate documented in the README. Pull
requests should state the user-visible outcome, architectural impact, test
evidence, and whether real hardware was exercised.

Physical panel evidence must name the panel profile but must not expose its
local secrets or imply that a single panel result generalizes to all firmware,
temperatures, or display batches.
