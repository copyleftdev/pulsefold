# Deterministic simulation

`pulsefold sim` generates operations from a seed, runs them against one implementation and the reference, checks properties at every step, and shrinks the first failure to a replayable assay. The approach follows TigerBeetle's VOPR (seeded runs, swarm testing) and Antithesis-style properties (always/sometimes).

    pulsefold sim [--variant V] [--seed N] [--runs K] [--ops L] [--shrink-to DIR]

Defaults: `--variant indexed --seed 1 --runs 100 --ops 48`. Seeds `N..N+K-1` run in order.

| Exit | Meaning |
|---|---|
| 0 | No failure, and every sometimes-property fired. |
| 1 | A failure was found. The report names the seed, the reason, the shrunk operations and the command that reproduces it. |
| 2 | Usage error. |
| 3 | **Weak:** no failure, but some sometimes-property never fired, so a green result says little. |

## Seeds and swarm

The generator is SplitMix64 with Lemire's unbiased range reduction (`engine/src/rng.rs`), so a seed means the same run on every platform. Each seed first draws its own knobs: how many booking IDs are in play, how often an operation is a move, how often a window is placed against an existing booking, and how wide the time region is. Different seeds stress different mixes instead of one average mix. Operations are a pure function of the seed, and a shorter run is a prefix of a longer one. Tests pin the first operations of seed 1 and a fingerprint of seeds 1–100, so a generator change cannot silently change what a published seed means.

## Properties

**Always:** `agrees_with_reference`. After every operation, the implementation's result, booking list and availability match the reference. Availability is probed in the moved booking's original window and the attempted window; if both agree, a sweep probes every interval between consecutive endpoints of the reference's bookings and the operation's windows.

**Sometimes** (each must fire at least once per report):

| Property | Fires when |
|---|---|
| `rejected_move` | a move is rejected with `CONFLICT` |
| `stale_write_observable` | a stale index write on that rejected move would change some availability answer |
| `stale_write_hidden_from_named_probes` | ...but neither the full original nor the full attempted window would show it |
| `valid_move` | a move is accepted |
| `move_after_rejection` | a booking moves after one of its moves was rejected |
| `touching_accept` | an operation touching another booking end-to-start is accepted |
| `duplicate_rejected` | a reserve of an existing ID is rejected |
| `missing_move` | a move of an unknown ID is rejected |
| `chamber_full` | no chamber is free in a probed window |

Sometimes-properties are judged from the reference alone, so they count the same situations for every variant.

## Shrinking

The first failing seed is cut at its failing operation, then minimized: delete chunks of operations, fold a move into the reserve of the same booking, repeat until nothing changes, and rename IDs to `A`, `B`, ... in order of appearance. Every step keeps the failure. `--shrink-to DIR` writes `seed-N.assay` and a one-turn `seed-N.session`; `pulsefold trace DIR/seed-N.session` produces a trace v2 file the browser console imports. A simulation has no agent response, so its marker is `UNOBSERVED`.

## What it found about the stale-index bug

Measured with `pulsefold sim --variant stale-index --seed 1 --runs 1000 --ops 64`:

- 859 of 1000 seeds fail. The others never produce an observable stale write.
- The full **attempted** window never catches it: the conflicting booking keeps that chamber busy there either way.
- The full **original** window reports the failure first in 691 seeds (80%). It misses when the attempted window overlaps the original one (the stale entry still covers part of it) or another booking covers it.
- The remaining 168 (20%) are visible only to the sweep, in the vacated or newly claimed part of the window. This matches the independently computed `stale_write_hidden_from_named_probes` rate for correct runs (1126 of 5655 observable stale writes).
- A stale write can be **latent**: if other bookings cover everything it changed, no availability query can see it, and it surfaces operations later when that cover moves. A test checks that every late catch follows only invisible stale writes, so the sweep never misses an observable one.

All three correct variants (`scan`, `indexed`, `rebuild`) pass 1000 seeds with every sometimes-property reached.

## Limits

The sweep only probes endpoints taken from the reference and the operation. That is complete for stale index entries, whose endpoints always come from those sets; an implementation that invents new endpoints could slip between probes. There is one planted bug, one process, and no concurrency yet. Results describe this simulator, not agents.
