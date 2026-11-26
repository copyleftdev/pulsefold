// Native/WebAssembly parity: every request must give the same status and the same bytes.
// Usage: node parity.mjs NATIVE_BINARY WASM_FILE [WASM_FILE...]
import {readFileSync} from 'node:fs';
import {spawnSync} from 'node:child_process';
import assert from 'node:assert/strict';
import {load} from '../dist/engine.mjs';

const [native, ...wasms] = process.argv.slice(2);
assert.ok(native && wasms.length, 'usage: node parity.mjs NATIVE_BINARY WASM_FILE...');
const race = readFileSync(new URL('sim/race-seed-257.race', import.meta.url), 'utf8');
const requests = [
  'sim stale-index 1 48 200', 'sim indexed 1 48 200', 'sim closed-interval 7 64 50', 'sim scan 3 16 1',
  'race check-then-act 1 1000', 'race torn-move 1 1000', 'race locked 1 200', 'race locked-writes 9 1',
  'replay\n' + race, 'replay\n' + race.replace('variant torn-move', 'variant locked'),
  'sim nope 1 1 1', 'replay\nvariant locked', '',
];
const expected = requests.map(r => {
  const p = spawnSync(native, ['api'], {input: r, encoding: 'utf8'});
  return {code: p.status, out: p.stdout};
});
for (const file of wasms) {
  const engine = await load(readFileSync(file));
  requests.forEach((r, i) => {
    const got = engine.call(r);
    assert.equal(got.code, expected[i].code, `${file}: status for ${JSON.stringify(r.slice(0, 40))}`);
    assert.equal(got.out, expected[i].out, `${file}: output for ${JSON.stringify(r.slice(0, 40))}`);
  });
}
console.log(`Parity: ${requests.length} requests identical across native and ${wasms.length} WebAssembly build(s).`);
