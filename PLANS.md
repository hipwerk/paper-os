# Execution plans

Use an execution plan for work spanning multiple crates, a public API change, a
new hardware backend, or a physical-panel experiment. Keep it in
`docs/plans/<short-name>.md` while active; move durable conclusions into an ADR
and delete or archive stale mechanics when done.

```md
# Outcome

What observable result will exist?

## Context and constraints

Relevant docs, crate boundaries, panel limitations, and explicit non-goals.

## Decisions

Choices already made and choices that still need evidence.

## Work

- [ ] Small, verifiable step
- [ ] Small, verifiable step

## Verification

Exact automated commands, preview/golden evidence, and—only when authorized—the
panel profile and physical lab procedure.

## Risks and rollback

Panel-health hazards, compatibility risks, migration impact, and how to revert.
```
