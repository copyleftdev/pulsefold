# Pulsefold: The Ghost Cycle

**A playable debugger for testing whether an AI agent's instruction canary tells us anything about the correctness of the software it writes.**

A fictional orbital habitat schedules the chamber cycles that keep its artificial metabolism running. At the console, Cycle A is moved toward a slot already held by Cycle B. The move correctly returns `CONFLICT` and the booking ledger stays unchanged. In the faulty replay, the chamber index changes anyway. The console gives the wrong availability answer while the canary remains exact.

That's the reason this project exists: a model can print the correct marker while its code quietly breaks a rule. Pulsefold puts that difference on screen and makes the sequence replayable. Its subject is software scheduling, not laboratory or biochemical procedures.

## Start here

| If you want to… | Start with… |
|---|---|
| Play the scenario without installing Rust, Godot, or GPU tooling | Run `python3 -m http.server 8000 --bind 127.0.0.1 --directory dist`, then open [localhost:8000](http://127.0.0.1:8000). |
| Check the exported trace format and fixture expectations | Run `node checks.mjs` from this folder (Node 18+). |
| Build the Rust runner and check its traces against the fixtures | Run `cd engine && ./verify.sh` (stable Rust 1.65+ and a POSIX shell). |
| Use the optional native console | Open `engine/viewer/project.godot` in Godot 4. See [native setup](engine/README.md). |
| Pick up the next engineering task | Read [the project brief](docs/EXPERIMENT.md) and [the replay contract](docs/REPLAY.md). |

In the browser, choose **First failure** in **Stale index** mode, then choose **Control** and step through the same move. Both reject the move. Only the fault shows the ghost booking and fails the independent check. **Export replay** downloads the current trace. **Import TSV** reads a compatible file locally; it labels imported verdicts as recorded, not independently verified. The browser does not run the Rust scheduler.

## What is in this repository?

- `dist/`: buildless browser console; two illustrative fixtures, presentation, and replay controls.
- `engine/`: dependency-free Rust runner, starter Assay scenario, native Godot viewer, and a verification script.
- `checks.mjs`: automated browser-data checks for the packaged traces.
- `docs/`: experiment boundary, replay contract, and visual direction.

**Verified so far:** browser-preview play/step/rewind, first-failure jump, fault vs. control, and TSV export; Node replay checks; Godot headless loaded both packaged traces. **Not verified here:** Rust compilation, Rust-generated trace parity, GPU rendering, TLA+ model checking, and real agent data. The included traces are hand-authored synthetic fixtures, not measured experiment outcomes.

## Keep the two projects separate

Pulsefold is a companion experiment and debugger. [Canary Lab](https://github.com/copyleftdev/canary-article) remains the source for the existing study and its frozen cohorts. The merged [offline fixture](https://github.com/copyleftdev/canary-article/pull/2) and [room-index discussion](https://github.com/copyleftdev/canary-article/issues/3) provide context, not evidence that Pulsefold validates or replaces those studies. GnomeMan4201 proposed the room-index turn and contributed Canary Lab’s offline scorer; [SimTigerBeetle](https://tigerbeetle.com/blog/2023-07-11-we-put-a-distributed-database-in-the-browser/) inspired the playable replay approach.

## Create the private GitHub repository

Extract the ZIP, then from the new `pulsefold/` folder:

```sh
git init -b main
git add .
git diff --cached --stat
git commit -m "Initialize Pulsefold Ghost Cycle experiment"
gh repo create copyleftdev/pulsefold --private --source=. --remote=origin --push
gh repo view copyleftdev/pulsefold --json visibility,url
```

Use an already authenticated GitHub CLI on your own computer and confirm that it is signed in as `copyleftdev`. Stop if the repo already exists or GitHub shows a different owner or visibility. The ZIP does not include a Git remote, hosting credentials, or Site deployment settings. Leave the repo private and don’t post links or comments until the experimental scope is settled.
