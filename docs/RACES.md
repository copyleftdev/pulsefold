# Deterministic races

`pulsefold race` runs two or three clients against one store at once. Each operation is a short program of atomic micro-steps; a seeded scheduler chooses which client takes the next step. A seed fixes the scripts and the interleaving, so a race found once replays exactly, every time.

    pulsefold race [--variant V] [--seed N] [--runs K] [--shrink-to DIR]   # defaults: locked, 1, 200
    pulsefold race --replay FILE.race

Exit codes match `pulsefold sim`: 0 pass, 1 failure, 2 usage error, 3 weak (a sometimes-property that applies never fired).

## Model

Micro-steps: `acquire`, `check` (decide the result from the ledger), `write-ledger`, `unindex`, `index`, `reindex` (unindex and index in one step), `read` (availability from the index), `release`, `done`. A client blocked on `acquire` cannot be scheduled; if the scheduler picks it, the next client that can step does, and `lock_contention` fires. Client 0 runs the setup operations alone first. Clients also run `query START END`, an availability read.

| Variant | Kind | Reads | Writes | A move's index update |
|---|---|---|---|---|
| `locked` | control | hold the lock | hold the lock | unindex, then index |
| `locked-writes` | control | lock-free | hold the lock | one `reindex` step |
| `check-then-act` | bug | lock-free | **no lock** | one `reindex` step |
| `torn-move` | bug | lock-free | hold the lock | **unindex, then index** |

Each bug is one change away from `locked-writes`.

## Oracle: linearizability

Every run records, per operation, when it was invoked, when it responded, and what it returned. The history passes if some order of its operations exists that (1) respects real time: an operation that responded before another was invoked comes first; (2) reproduces every recorded answer on the sequential reference; and (3) ends in exactly the state the store ended in, bookings and availability on every interval. This is the check Jepsen's Knossos performs, by depth-first search with a memo of dead (remaining operations, reference state) pairs.

Condition (3) has to be part of the search. Two overlapping moves of one booking can both be ordered either way and match every answer, but only one order ends where the store did. Comparing the store against whichever order is found first flagged correct `locked` runs; the control caught it.

## Sometimes-properties

| Property | Fires when |
|---|---|
| `overlapping_ops` | two clients' operations overlap in time |
| `contended_write` | two overlapping writes target overlapping windows |
| `conflict_under_contention` | a write overlapping another write is rejected with CONFLICT |
| `lock_contention` | the scheduler picks a client blocked on the lock (not applicable to `check-then-act`, which has no lock) |
| `query_during_move` | a query reads while an accepted move is still in flight |

## Shrinking and replay

A failing scenario is minimized by removing setup operations, client operations and whole clients, by trying round-robin and two-block schedules after each removal, and by deleting schedule entries. Each accepted step must lower (operations, context switches, schedule length), so shrinking terminates. The schedule is replaced by the steps actually taken, so the result replays exactly. `--shrink-to DIR` writes a `.race` file:

```text
variant torn-move
setup reserve C 0 600 630
client 1 query 600 750
client 2 move C 645 660
schedule 2 2 2 2 1 1 2 2 2
```

`pulsefold race --replay` prints every micro-step, the history and the verdict. Here the query reads at time 11, between the move's `unindex` (10) and `index` (13), and sees both chambers free across 600–750: true neither before the move (C at 600–630) nor after it (C at 645–660). Changing the first line to `variant locked` makes the same scenario linearizable.

The shrunk `check-then-act` case is two clients reserving the same ID and slot: both `check` before either writes, and both return OK.

## Measured

`pulsefold race --variant V --runs 1000` (seeds 1–1000, 6648 operations in all; 95% Wilson intervals):

| Variant | Failing seeds | 95% CI | Failing with no preemption inside an operation | Shrunk to |
|---|---|---|---|---|
| `locked` | 0/1000 | 0.000–0.004 | 0/1000 | – |
| `locked-writes` | 0/1000 | 0.000–0.004 | 0/1000 | – |
| `check-then-act` | 50/1000 | 0.038–0.065 | 0/1000 | 2 clients, 2 operations |
| `torn-move` | 4/1000 | 0.002–0.010 | 0/1000 | 2 clients, 3 operations |

- **The bugs need preemption inside an operation.** Running the same scenarios with each operation atomic finds nothing, and a single client matches the sequential reference exactly for every variant (tested over 100 sequential seeds). Sequential tests cannot see these bugs.
- **`torn-move` is rare:** about 0.4% of seeds. At that rate, roughly 750 random seeds give a 95% chance of seeing it once. That is the case for guided exploration over uniform random scheduling, and the next lever here: a swarm knob or a sometimes-property-driven search toward queries that overlap a move.
- Controls reach every applicable sometimes-property in 1000 seeds.

## Limits

One lock, one store, two or three clients and at most three operations each, plus up to three setup reservations. The scheduler is uniform random with a per-seed switch rate, not PCT or coverage-guided. Linearizability search is exponential in the worst case; the memo keeps these history sizes fast (a test pins a 16-operation adversarial history under 5 s). The sweep-style final-state check uses the endpoints of the reference's and the index's own entries.
