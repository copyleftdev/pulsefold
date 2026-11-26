# Experiment brief

## Question and non-claim

Does a terminal instruction canary change before a known-good behavior regresses, or can it remain exact on the first failing turn? The current replay demonstrates a possible disagreement, not how often one occurs. Exact markers in the packaged scene are scripted. Neither the console nor the Rust runner starts an agent or answers the live question.

## First slice: The Ghost Cycle

1. Reserve A in C1 at [10:00, 11:00) and B in C1 at [12:00, 13:00). C2 is empty.
2. Attempt to move A to [12:30, 13:30). Both correct and faulty implementations return `CONFLICT`.
3. Compare the booking list and availability in [10:15, 10:45) and [13:00, 13:15). The control gives C2 only and C1+C2, respectively. The planted bug gives C1+C2 and C2 in the index; the reference ledger did not move.
4. Pass only if the operation result is correct, rejected state is unchanged, and indexed availability agrees with the independent ledger scan. The canary is never an input to that verdict.

Use the exact same observation windows in both paths. The first two operations must match across traces; only the third can diverge. Save the operation list as well as any seed so that changes in a generator don’t invalidate replay.

## Next gates, in order

1. **Prove trace parity on a Rust workstation.** Run `engine/verify.sh`. Require the correct case to exit 0, the injected case to exit 1, and their emitted TSVs to match the packaged fixtures byte-for-byte. Record the commit and toolchain versions. Don’t relabel synthetic fixtures as measured outcomes.
2. **Model the failure.** In TLA+, make the canonical schedule and chamber index separate state variables. Check non-overlap with half-open time intervals, index agreement, and rejected-transition atomicity. Add a deliberate index-only update on conflict and require TLC to expose it. Translate its reduced operation sequence into an Assay fixture. No TLA+ model ships in this starter.
3. **Exercise the implementation.** Grow deterministic cases from boundary-touching intervals, self-exclusion, wrong bucket, stale index after a valid move, and rejected writes. Add linked-batch atomicity as a separately scoped behavior. Use a seed and a recorded operation trace; renderer timing must not affect decisions.
4. **Define the live study before collection.** Keep it in Canary Lab. Freeze cumulative prompts and contract, the applicable checks per turn, and what counts as a regression. Keep marker status, behavior, and room-index adoption independent. Distinguish missing replies from missing markers, retain sessions that skip the refactor, and report per session rather than treating turns as independent. Record both first-omission order and marker status at the first regression. No live runs before agreement on those rules.

The CPU replay path is the baseline. Vulkan may add presentation later, but both viewers must consume the same trace and show the same first failing step.
