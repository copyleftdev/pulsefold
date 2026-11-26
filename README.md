# Pulsefold: The Ghost Cycle

**A playable debugger for testing whether an AI agent's instruction canary tells us anything about the correctness of the software it writes.**

A fictional orbital habitat schedules the chamber cycles that keep its artificial metabolism running. A scripted three-turn session replays the same operations against each turn's implementation and a separate reference. On turn 3 the implementation gains a room-keyed chamber index. Cycle A is moved toward a slot already held by Cycle B. The move correctly returns `CONFLICT` and the booking list stays unchanged. In the faulty session, the chamber index changes anyway, so availability in A's original window is wrong, while the canary remains exact.

That's the reason this project exists: a model can print the correct marker while its code quietly breaks a rule. Pulsefold puts that difference on screen and makes the sequence replayable. Its subject is software scheduling, not laboratory or biochemical procedures.

## Start here

| If you want to… | Start with… |
|---|---|
| Play the scenario without installing Rust, Godot, or GPU tooling | Run `python3 -m http.server 8000 --bind 127.0.0.1 --directory dist`, then open [localhost:8000](http://127.0.0.1:8000). |
| Check the exported trace format and fixture expectations | Run `node checks.mjs` from this folder (Node 18+). |
| Build the Rust runner and check its traces against the fixtures | Run `cd engine && ./verify.sh` (stable Rust 1.65+ and a POSIX shell). |
| Replay a session yourself | `cd engine && cargo run --offline -- trace sessions/ghost.session` or `-- summary sessions/ghost.session`. |
| Measure how well the simulator finds six planted bugs | `cd engine && cargo run --release --offline -- catalog`. See [the planted-bug catalog](docs/SIMULATION.md#planted-bug-catalog). |
| Hunt for the bug from seeds instead of a script | `cd engine && cargo run --offline -- sim --variant stale-index --runs 1000 --ops 64 --shrink-to /tmp/pf`, then import the trace of `/tmp/pf/seed-2.session`. See [the simulation guide](docs/SIMULATION.md). |
| Use the optional native console | Open `engine/viewer/project.godot` in Godot 4. See [native setup](engine/README.md). |
| Pick up the next engineering task | Read [the project brief](docs/EXPERIMENT.md) and [the replay contract](docs/REPLAY.md). |

In the browser, choose **First failure** in **Stale index** mode, then choose **Control** and step through the same move. Both reject the move. Only the fault shows the ghost booking and fails the independent check, and only the original-window probe catches it. **Export replay** downloads the current trace. **Import TSV** reads a compatible file locally; it labels imported verdicts as recorded, not independently verified. The browser does not run the Rust scheduler.

## What is in this repository?

- `dist/`: buildless browser console; the two runner-generated traces, presentation, and replay controls.
- `engine/`: dependency-free Rust runner (reference, implementation variants, marker scoring, session summary, seeded simulation with always/sometimes properties and shrinking), the Ghost Cycle assay, five scripted sessions with expected summaries, golden simulation reports, native Godot viewer, and a verification script.
- `checks.mjs`: automated browser-data checks for the packaged traces.
- `docs/`: experiment boundary, replay contract, simulation guide, and visual direction.

**Verified so far (2026-11-26):** `engine/verify.sh` passes on rustc 1.99.0 with Godot 4.7.2: 45 Rust tests, golden simulation and catalog reports, the shrunk replay, runner-generated traces byte-identical to `dist/` and `engine/viewer/`, all five session summaries, and headless Godot validation (which rejects swapped or v1 traces). The tests also pass on rustc 1.73.0. `node checks.mjs` passes. The browser console was exercised at desktop and phone widths. **Not verified:** the declared Rust 1.65 minimum itself, GPU rendering, TLA+ model checking, and real agent data. The sessions are scripted, so the traces are not measured experiment outcomes.

## Keep the two projects separate

Pulsefold is a companion experiment and debugger. [Canary Lab](https://github.com/copyleftdev/canary-article) remains the source for the existing study and its frozen cohorts. The merged [offline fixture](https://github.com/copyleftdev/canary-article/pull/2) and [room-index discussion](https://github.com/copyleftdev/canary-article/issues/3) provide context, not evidence that Pulsefold validates or replaces those studies. GnomeMan4201 proposed the room-index turn and contributed Canary Lab’s offline scorer; [SimTigerBeetle](https://tigerbeetle.com/blog/2023-07-11-we-put-a-distributed-database-in-the-browser/) inspired the playable replay approach.

## License

[MIT](LICENSE), the same as Canary Lab.
