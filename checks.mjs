// Trace v2 contract checks for the packaged browser fixtures (Node 18+).
import {readFileSync} from 'node:fs';
import assert from 'node:assert/strict';
import {parse,firstFailure,seek,domain,header} from './dist/replay.mjs';
const read=f=>readFileSync(f,'utf8');
const a=read('dist/control.tsv'),b=read('dist/mutant.tsv');
assert.equal(a,read('engine/viewer/control.tsv'),'browser and native control traces differ');
assert.equal(b,read('engine/viewer/mutant.tsv'),'browser and native fault traces differ');
const control=parse(a),mutant=parse(b);
assert.equal(control.length,9);assert.equal(mutant.length,9);
assert.deepEqual(control.slice(0,6),mutant.slice(0,6),'turns 1-2 are identical');
assert.deepEqual(mutant.map(r=>r.turn),[1,1,1,2,2,2,3,3,3]);
assert.equal(firstFailure(control),-1);
const at=firstFailure(mutant),bad=mutant[at];
assert.equal(at,8);assert.equal(mutant.filter(r=>r.verdict==='FAIL').length,1);
assert.equal(bad.variant,'stale-index');assert.equal(bad.refactor,'used');
assert.equal(bad.result,'CONFLICT');assert.equal(bad.expected,'CONFLICT');assert.equal(bad.marker,'EXACT');
assert.deepEqual(bad.ledger,control[8].ledger,'the booking list still looks right');
assert.notDeepEqual(bad.ledger,bad.index,'the index holds the ghost');
assert.deepEqual(bad.original,{lo:600,hi:660,ref:'C2',impl:'C1,C2'},'original window catches it');
assert.deepEqual(bad.attempted,{lo:750,hi:810,ref:'C2',impl:'C2'},'attempted window masks it');
assert.match(bad.reason,/^original window 600-660/);
assert.equal(mutant[0].original,null);
assert.deepEqual(domain(mutant),[600,840]);assert.equal(seek(mutant,80),8);assert.equal(seek(mutant,-8),0);
// Malformed input is rejected.
const v1='step\toperation\tresult\tledger\tindex\toracle_old\tindex_old\toracle_new\tindex_new\tcanary\tverdict\n1\treserve A 0 600 660\tOK\tA@C1:600-660\tA@C1:600-660\tC2\tC2\tC1,C2\tC1,C2\tEXACT\tPASS';
for(const broken of [v1,header,b.replace('A@C1:600-660','<svg>'),b.replace('\tEXACT\t','\tWRONG\t'),
  b.replace('\tPASS\t-','\tPASS\tlooks fine'),b.replace(/\tFAIL\t.*$/m,'\tFAIL\t-'),b.replace('\n2\t4\t','\n3\t4\t'),
  b.replace('\t600-660\tC2\tC1,C2\t','\t660-600\tC2\tC1,C2\t'),b.replace('\tused\t','\tmaybe\t'),b.replace('\tstale-index\t','\tfast\t')])
  assert.throws(()=>parse(broken));
assert.deepEqual(parse(b.replaceAll('\n','\r\n')),mutant);
// The packaged WebAssembly engine reproduces the native golden reports and replays.
const {load,SEP}=await import('./dist/engine.mjs');
const engine=await load(readFileSync('dist/pulsefold.wasm'));
const golden=f=>read('engine/sim/'+f);
let r=engine.call('sim stale-index 1 48 200');
assert.equal(r.code,1);assert.equal(r.out.split(SEP)[0],golden('stale-index.expected'));
const live=parse(r.out.split(SEP)[1]);assert.equal(live.length,3);assert.equal(firstFailure(live),2);
assert.equal(engine.call('sim indexed 1 48 200').out.split(SEP)[0],golden('indexed.expected'));
for(const v of ['locked','locked-writes','check-then-act','torn-move']){
  r=engine.call(`race ${v} 1 1000`);assert.equal(r.out.split(SEP)[0],golden(`race-${v}.expected`),v);
}
const raceFile=golden('race-seed-257.race');
assert.equal(engine.call('replay\n'+raceFile).code,1);
assert.equal(engine.call('replay\n'+raceFile.replace('variant torn-move','variant locked')).code,0);
assert.equal(engine.call('fly').code,2);
// Flight recorder frames: what the game draws, derived only from engine output.
const F=await import('./dist/frames.mjs');
const fault=F.traceFrames(mutant),calm=F.traceFrames(control);
assert.deepEqual(fault[8].ghosts,[{id:'A',room:1,lo:750,hi:810}]);
assert.deepEqual(fault[8].missing,[{id:'A',room:1,lo:600,hi:660}]);
assert.deepEqual(fault[8].probes.map(p=>[p.name,p.agree]),[['original',false],['attempted',true]]);
assert.ok(calm.every(f=>!f.ghosts.length&&!f.missing.length&&f.probes.every(p=>p.agree)));
assert.deepEqual(F.domainOf(fault),[600,840]);
assert.deepEqual(F.sweepProbe('result CONFLICT, reference OK; sweep window 765-795 available C1,C2, reference C1'),{name:'sweep',lo:765,hi:795,impl:'C1,C2',ref:'C1',agree:false});
assert.equal(F.sweepProbe('original window 1-2 available C1, reference C2'),null);
const torn=F.raceFrames(engine.call('replay\n'+raceFile).out);
assert.equal(torn.failed,true);assert.equal(torn.frames.length,15);
const tornRead=torn.frames.find(f=>f.micro==="read");
assert.equal(tornRead.time,11);assert.equal(tornRead.lock,"2");
assert.deepEqual(tornRead.probes,[{name:'read',lo:600,hi:750,impl:'C1,C2',ref:'C2',agree:false}]);
assert.deepEqual(tornRead.missing.map(s=>s.id),['C']);
const fixed=F.raceFrames(engine.call('replay\n'+raceFile.replace('variant torn-move','variant locked')).out);
assert.equal(fixed.failed,false);
assert.ok(fixed.frames.filter(f=>f.micro==='read').every(f=>f.probes[0].agree&&!f.missing.length&&!f.ghosts.length));
assert.throws(()=>F.raceFrames('variant locked\n'));
console.log('Replay contract checks passed.');
