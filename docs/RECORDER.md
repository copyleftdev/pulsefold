# Flight recorder

`dist/index.html` is the game-style front end: a full-screen canvas with a HUD over it. It runs the Rust engine (WebAssembly) in the tab and never computes a verdict itself. The table-based console is still at `dist/console.html`.

## The picture

The habitat is seen from above. Mission time wraps clockwise around a dial; chamber C1 is the inner ring, C2 the outer.

| On the dial | Means |
|---|---|
| Solid band with an ID | A booking in the reference ledger |
| Bright thin trace on a band | The implementation's index entry for that booking |
| Hatched coral band, `GHOST` tag | An index entry with no booking behind it |
| Dashed band, `UNINDEXED` tag | A booking with no index entry |
| Translucent wedge across both rings | A probe window, tagged with both answers; coral and `≠` when they disagree, amber edge when the operation was rejected |
| Hub | Turn, step and ✓/× for sessions; the lock and its holder for races |
| Diamonds outside the dial | Race clients, parked at the window they are working on |

## Missions

1. **The Ghost Cycle** (scripted): the packaged three-turn session. Toggle the fault and the control.
2. **Seed Hunt** (live): pick a planted bug or the correct index. The engine searches seeds in chunks of 50, shrinks the first failure and replays it. A correct implementation reports a clean sweep of 5000 seeds.
3. **Race Condition** (live): search seeds for a non-linearizable interleaving, autoplay it, and stop on the torn read. **Apply lock** replays the same interleaving with every operation holding the lock.
4. **Multiverse** (live): run 250, 500 or 1000 seeds at once, for any simulation implementation or race variant. Each universe is an orb: a coral core with a fine ring if the bug surfaced, slate if it held; the thin arc around it is the share of sometimes-properties it reached (all of them, segment by segment, on hover or selection). **Arrange by** regroups the orbs by a swarm knob or by outcome, with a failure count and rate bar per group, so you can see which parameters find a bug. **Failing universes only** dims the rest. Click a universe, or use the arrow keys and Enter, to see its knobs, outcome and properties, and **Replay in recorder** to watch it (shrunk first if it failed); **Back to multiverse** keeps the field as it was.

The multiverse uses the engine's `universes` request, one row per seed with no shrinking, so 1000 universes come back in about 0.1 s. A Rust test checks that the rows add up to the aggregate `sim` and `race` reports.

## Motion rules

From the visual direction: motion never carries data. Every settled frame holds the whole truth, and step, rewind and jump work paused. The radar sweep, ripples, shockwaves, sparks, the 2 px shake, the mint wave and the starfield are decoration. The motion toggle (persisted per browser) and `prefers-reduced-motion` turn them off; banners then appear without animation. Sound is synthesized, quiet, and off until the viewer turns it on.

## Keys

`1`–`4` missions · `←`/`→` step · `Space` play · `Home` rewind · `F` first failure · `Esc` back one level · `M` sound. In the multiverse, arrow keys move between universes on screen and `Enter` replays the selected one.

## Code

- `frames.mjs`: pure adapters from engine output (trace v2, race narration) to frames: ghosts, unindexed bookings, probes (including the sweep's disagreement and what a race read saw versus the ledger), lock holder, captions. Tested in `checks.mjs`.
- `scene.mjs`: the canvas renderer. Arcs glide between frames; the canvas only animates while something moves.
- `fx.mjs`: easing, motion preference, sound.
- `game.mjs`: missions, searches, playback, HUD and keys.
- `field.mjs` and `multiverse.mjs`: the multiverse renderer and its controls.
