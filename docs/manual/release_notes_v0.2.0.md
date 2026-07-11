# Nebula v0.2.0

Release date: 2026-07-11

## Highlights
- Multi-replica correctness on Node and Scheduler (per-replica identity, scale-in, Drain).
- Recovery budget and safer local engine restart cleanup.
- Router plan-version consistency so scale-in does not drop the remaining ready endpoints.
- Documentation reorganized under `docs/arch/`, `docs/dev/`, and `docs/manual/`.

## Validation
- Unit tests for scheduler scale helpers and related packages.
- Remote smoke on 8×5090: dual-replica Gemma-4-31B (TP=4) → scale to 1 → chat via Gateway.
