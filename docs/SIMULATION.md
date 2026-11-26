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

## Planted-bug catalog

    pulsefold catalog [--seed N] [--runs K] [--ops L]     # defaults: 1, 1000, 64

Each row runs the same seeds against one implementation. Exit 0 means every bug was found at least once and no correct implementation (control) was ever flagged.

| Variant | Kind | What it gets wrong | Minimal witness |
|---|---|---|---|
| `scan`, `indexed`, `rebuild` | control | nothing | |
| `stale-index` | bug | a rejected move still writes the attempted slot into the index | 3 ops |
| `leftover-entry` | bug | a valid move adds the new index entry but leaves the old one | 2 ops |
| `closed-interval` | bug | index lookups treat touching intervals as overlapping | 2 ops |
| `wrong-bucket` | bug | every booking is filed under the first chamber's index list | 1 op |
| `index-before-check` | bug | a reserve writes the index before the duplicate and conflict checks | 2 ops |
| `no-self-exclusion` | bug | a move conflicts with the booking's own current slot (not index-related; a contrast) | 2 ops |

The index is a list of entries per chamber, as agent code usually writes it, so a leftover or misfiled entry can exist instead of being silently overwritten. Each bug has a hand-written minimal witness in the tests that fails on it and passes on `indexed`.

Measured with `pulsefold catalog --runs 1000 --ops 64` (rates per seed; 95% Wilson intervals; "op" is the operation number of the first failure):

| Variant | Detected | 95% CI | Median op | p90 op | Named probes only | Median op, named only | Shrunk | First check |
|---|---|---|---|---|---|---|---|---|
| `scan` | 0/1000 | 0.000–0.004 | – | – | 0/1000 | – | – | – |
| `indexed` | 0/1000 | 0.000–0.004 | – | – | 0/1000 | – | – | – |
| `rebuild` | 0/1000 | 0.000–0.004 | – | – | 0/1000 | – | – | – |
| `stale-index` | 859/1000 | 0.836–0.879 | 20 | 43 | 850/1000 | 21 | 3 | original 691, sweep 168 |
| `leftover-entry` | 999/1000 | 0.994–1.000 | 8 | 19 | 998/1000 | 9 | 2 | original 701, sweep 298 |
| `closed-interval` | 1000/1000 | 0.996–1.000 | 3 | 7 | 999/1000 | 8 | 2 | sweep 835, attempted 150, original 15 |
| `wrong-bucket` | 943/1000 | 0.927–0.956 | 3 | 16 | 943/1000 | 3 | 1 | attempted 943 |
| `index-before-check` | 1000/1000 | 0.996–1.000 | 6 | 19 | 1000/1000 | 7 | 2 | attempted 621, sweep 358, original 21 |
| `no-self-exclusion` | 967/1000 | 0.954–0.976 | 14 | 36 | 967/1000 | 14 | 2 | result 967 |

What the table supports:

- **No false positives:** 0 of 3000 control runs flagged; each control's 95% upper bound is 0.4%.
- **Misses are coverage, not blindness, where tested.** For `wrong-bucket` and `no-self-exclusion`, a test checks all 1000 seeds: a seed is detected exactly when the generator triggered the bug (an accepted reserve in C2; a move overlapping its own slot that the reference accepts). The 141 `stale-index` misses are seeds with no observable stale write or with one still latent when the run ended; the tests do not separate those two.
- **The sweep mostly buys speed.** With only the two named windows, almost the same seeds are caught eventually (850 vs 859 for `stale-index`), because latent damage surfaces later. The clearest gain is `closed-interval`: median first detection at operation 3 with the sweep, 8 without. A test checks per seed that the full oracle never detects later than the named-only one.
- These are rates for this generator, run length and swarm, not for agent-written code.

## Limits

The sweep only probes endpoints taken from the reference and the operation. That is complete for stale index entries, whose endpoints always come from those sets; an implementation that invents new endpoints could slip between probes. There are six planted bugs, one process, and no concurrency yet. Results describe this simulator, not agents.
