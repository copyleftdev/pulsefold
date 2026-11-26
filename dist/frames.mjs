// Pure adapters from engine output to the frames the flight recorder draws. No DOM, no
// canvas: checks.mjs tests these in Node.
import {slots} from './replay.mjs';

export const clock=t=>Math.floor(t/60)+':'+String(t%60).padStart(2,'0');
const span=(lo,hi)=>`${clock(lo)}–${clock(hi)}`;
const rooms=v=>v==='none'?'none':v.replaceAll(',',' + ');

/** The window an operation acts on: `reserve A 0 600 660`, `move A 600 660`, `query 600 660`. */
export function opWindow(op){
  const w=op.split(' ');
  const [lo,hi]=w[0]==='reserve'?[w[3],w[4]]:w[0]==='move'?[w[2],w[3]]:[w[1],w[2]];
  return {lo:+lo,hi:+hi};
}

export function opTitle(op){
  const w=op.split(' ');
  if(w[0]==='reserve')return `Reserve ${w[1]} in C${+w[2]+1} at ${span(+w[3],+w[4])}`;
  if(w[0]==='move')return `Move ${w[1]} to ${span(+w[2],+w[3])}`;
  if(w[0]==='query')return `Query ${span(+w[1],+w[2])}`;
  return op;
}

/** Index entries with no matching booking (ghosts) and bookings with no matching entry. */
export function divergence(ledger,index){
  const same=(a,b)=>a.id===b.id&&a.room===b.room&&a.lo===b.lo&&a.hi===b.hi;
  const pool=[...index];
  const missing=[];
  for(const b of ledger){const i=pool.findIndex(e=>same(e,b));if(i<0)missing.push(b);else pool.splice(i,1)}
  return {ghosts:pool,missing};
}

const RESULT={OK:'accepted',CONFLICT:'rejected, conflict',DUPLICATE:'rejected, duplicate ID',NOT_FOUND:'rejected, no such booking'};
export const resultText=r=>RESULT[r]??r.toLowerCase();

/** The sweep's first disagreement, from a reason like `sweep window 825-840 available C1, reference C1,C2`. */
export function sweepProbe(reason){
  const m=/sweep window (\d+)-(\d+) available ([^,;]+(?:,C\d)?), reference ([^;]+?)(?:;|$)/.exec(reason??'');
  return m?{name:'sweep',lo:+m[1],hi:+m[2],impl:m[3].trim(),ref:m[4].trim(),agree:false}:null;
}

/** Frames for a trace v2 (parsed by replay.mjs): one per operation. */
export function traceFrames(rows){
  return rows.map(r=>{
    const probes=[];
    if(r.original)probes.push({name:'original',lo:r.original.lo,hi:r.original.hi,ref:r.original.ref,impl:r.original.impl});
    probes.push({name:'attempted',lo:r.attempted.lo,hi:r.attempted.hi,ref:r.attempted.ref,impl:r.attempted.impl});
    for(const p of probes)p.agree=p.ref===p.impl;
    const sweep=r.verdict==='FAIL'?sweepProbe(r.reason):null;
    if(sweep)probes.push(sweep);
    const title=opTitle(r.op);
    let caption;
    if(r.verdict==='FAIL')caption=`${title}: ${resultText(r.result)}. The implementation disagrees with the reference: ${r.reason}.`;
    else if(r.result==='CONFLICT')caption=`${title}: rejected, and nothing moved.`;
    else caption=`${title}: ${resultText(r.result)}.`;
    return {kind:'op',turn:r.turn,step:r.n,variant:r.variant,op:r.op,title,result:r.result,expected:r.expected,
      ledger:r.ledger,index:r.index,probes,verdict:r.verdict,reason:r.reason,marker:r.marker,refactor:r.refactor,caption,
      ...divergence(r.ledger,r.index)};
  });
}

const STEP_TEXT={
  acquire:'takes the lock',check:'checks the ledger','write-ledger':'writes the ledger',unindex:'removes its index entry',
  index:'adds its index entry',reindex:'replaces its index entry',read:'reads availability from the index',release:'releases the lock',done:'returns',
};

/** Chambers free in a window according to a list of bookings, as the engine writes them. */
export function freeRooms(bookings,w){
  const free=[1,2].filter(room=>!bookings.some(b=>b.room===room&&b.lo<w.hi&&w.lo<b.hi));
  return free.length?free.map(r=>'C'+r).join(','):'none';
}

/** Frames for a race narration (`pulsefold race --replay`): one per micro-step. */
export function raceFrames(text){
  const lines=text.trimEnd().split('\n');
  let variant='',verdict='',part='head';
  const events=[],history=[];
  for(const l of lines){
    const c=l.split('\t');
    if(c[0]==='variant'&&part==='head'){variant=c[1];continue}
    if(c[0]==='time'){part='events';continue}
    if(c[0]==='history'){part='history';continue}
    if(c[0]==='verdict'){verdict=c[1];continue}
    if(part==='events'){
      if(c.length!==7)throw Error('Unexpected race narration.');
      events.push({time:+c[0],client:c[1],op:c[2],step:c[3],note:c[4],index:slots(c[5]),ledger:slots(c[6])});
    }
    if(part==='history')history.push({client:c[1],op:c[2],invoked:+c[3],responded:+c[4],answer:c[5]});
  }
  if(!variant||!verdict||!events.length)throw Error('Incomplete race narration.');
  let lock=null;
  const doing={},last={};
  const frames=events.map((e,i)=>{
    if(e.step==='acquire')lock=e.client;
    if(e.step==='release')lock=null;
    doing[e.client]=e.step==='done'?null:e.op;last[e.client]=e.op;
    const who=e.client==='setup'?'Setup':`Client ${e.client}`;
    let caption=`${who} ${STEP_TEXT[e.step]??e.step}`;
    if(e.step==='check')caption+=`: ${e.note}`;
    if(e.step==='read')caption+=`: ${rooms(e.note)} free in ${span(...Object.values(opWindow(e.op)))}`;
    caption+=` (${opTitle(e.op).toLowerCase()}).`;
    // A read is a probe: what it saw from the index, against the ledger at that instant.
    const probes=[];
    if(e.step==='read'){const w=opWindow(e.op),ref=freeRooms(e.ledger,w);probes.push({name:'read',...w,impl:e.note,ref,agree:ref===e.note})}
    return {kind:'micro',index:e.index,ledger:e.ledger,time:e.time,step:i+1,client:e.client,op:e.op,micro:e.step,note:e.note,
      lock,active:{...doing},last:{...last},caption,probes,...divergence(e.ledger,e.index)};
  });
  return {variant,frames,history,verdict,failed:verdict!=='linearizable'};
}

/** A padded time range covering everything the frames draw. */
export function domainOf(frames){
  let lo=Infinity,hi=-Infinity;
  const take=w=>{lo=Math.min(lo,w.lo);hi=Math.max(hi,w.hi)};
  for(const f of frames){
    for(const s of [...f.ledger,...f.index])take(s);
    for(const p of f.probes??[])take(p);
    if(f.op)take(opWindow(f.op));
  }
  if(!isFinite(lo))return [600,840];
  lo=Math.floor(lo/60)*60;hi=Math.ceil(hi/60)*60;
  if(hi-lo<240){const pad=(240-(hi-lo))/2;lo-=Math.floor(pad/60)*60;hi=lo+240}
  return [Math.max(0,lo),hi];
}
