# Working on Pulsefold

Read README.md and docs/EXPERIMENT.md first. This is a companion project, separate from Canary Lab's existing cohorts.

- Keep simulator verdicts and canary scores independent.
- The browser runs engine logic only through `dist/pulsefold.wasm`, built from `engine/`; never reimplement checks in JavaScript. After engine changes, rebuild it and keep `engine/parity.mjs` passing.
- In `dist/`, motion is decoration: every settled frame must carry the full data, and reduced motion must lose nothing (`docs/RECORDER.md`). Insert engine output as text only.
- Run node checks.mjs for replay or browser changes. Run engine/verify.sh for engine changes on a Rust workstation; report unavailable checks accurately.
- Do not post public issues or comments, deploy a Site, or start live or paid model runs without Don's explicit authorization.
