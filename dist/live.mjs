// The live engine panel: the Rust engine (WebAssembly) runs simulations and races in this tab.
// Engine output is only ever inserted as text.
import {load,SEP} from './engine.mjs';
const $=id=>document.getElementById(id);
const el=(tag,text,cls)=>{const n=document.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n};
const int=(id,lo,hi)=>{const v=Number($(id).value);if(!Number.isInteger(v)||v<lo||v>hi)throw Error(`${$(id).closest('label').firstChild.textContent.trim()} must be ${lo}–${hi}.`);return v};
const fields=text=>Object.fromEntries(text.trim().split('\n').map(l=>{const i=l.indexOf('\t');return [l.slice(0,i),l.slice(i+1)]}));

function summary(report,kind){
  const f=fields(report);
  const lines=[`${f.variant}: ${f.result}`];
  if(kind==='sim'){lines.push(`failing seeds ${f.failing_seeds}`)}else{lines.push(`failing seeds ${f.failing_seeds}`,`without preemption inside an operation: ${f.failing_seeds_serial.split('\t')[0]}`)}
  if(f.first_failure&&f.first_failure!=='-')lines.push(`first failure: ${f.first_failure}`,`reason: ${f.reason}`,`shrunk: ${f.shrunk}`,`reproduce: ${f.reproduce}`);
  if(f.unreached&&f.unreached!=='-')lines.push(`never reached: ${f.unreached}`);
  return lines.join('\n');
}

function parseNarration(text){
  const lines=text.trimEnd().split('\n'), events=[], history=[];let verdict='';let part='head';
  for(const l of lines){
    const c=l.split('\t');
    if(c[0]==='time'){part='events';continue}
    if(c[0]==='history'){part='history';continue}
    if(c[0]==='verdict'){verdict=c[1];continue}
    if(part==='events')events.push({time:c[0],client:c[1],op:c[2],step:c[3],note:c[4],index:c[5]});
    if(part==='history')history.push({client:c[1],op:c[2],invoked:c[3],responded:c[4],answer:c[5]});
  }
  return {events,history,verdict};
}

export async function initLive({showTrace}){
  let engine;
  try{engine=await load('pulsefold.wasm')}catch(err){$('engine-status').textContent='Engine unavailable: '+err.message;return}
  $('engine-status').textContent='RUST → WEBASSEMBLY · RUNS IN THIS TAB';
  for(const id of ['sim-run','race-run','race-replay'])$(id).disabled=false;
  const tab=which=>{
    $('tab-sim').setAttribute('aria-pressed',which==='sim');$('tab-race').setAttribute('aria-pressed',which==='race');
    $('sim-panel').hidden=which!=='sim';$('race-panel').hidden=which!=='race';
  };
  $('tab-sim').onclick=()=>tab('sim');$('tab-race').onclick=()=>tab('race');

  const busy=(button,work)=>{button.disabled=true;button.setAttribute('aria-busy','true');setTimeout(()=>{try{work()}finally{button.disabled=false;button.removeAttribute('aria-busy')}},20)};

  $('sim-run').onclick=()=>busy($('sim-run'),()=>{
    try{
      const v=$('sim-variant').value,seed=int('sim-seed',0,2**53),ops=int('sim-ops',1,2000),runs=int('sim-runs',1,5000);
      const {code,out}=engine.call(`sim ${v} ${seed} ${ops} ${runs}`);
      if(code===2)throw Error(out.replace(/^error: /,''));
      const [report,trace]=out.split(SEP);
      $('sim-report').textContent=summary(report,'sim');
      const f=fields(report);
      const label=f.first_failure&&f.first_failure!=='-'
        ?`LIVE · ${v.toUpperCase()} · SHRUNK FAILURE FROM ${f.first_failure.split(' (')[0].toUpperCase()} · RUST ENGINE IN THIS TAB`
        :`LIVE · ${v.toUpperCase()} · SEED ${seed}, NO FAILURE IN ${runs} SEEDS · RUST ENGINE IN THIS TAB`;
      showTrace(trace,label);
    }catch(err){$('sim-report').textContent=err.message}
  });

  let lane={events:[],at:0};
  const renderLanes=()=>{
    const {events,at}=lane, table=$('lanes');table.replaceChildren();
    if(!events.length)return;
    const order=c=>c==='setup'?0:Number(c);
    const clients=[...new Set(events.map(e=>e.client))].sort((a,b)=>order(a)-order(b));
    const head=el('tr');head.append(el('th','time'),...clients.map(c=>el('th',c==='setup'?'setup':'client '+c)),el('th','index after the step'));
    table.append(el('thead'));table.firstChild.append(head);
    const body=el('tbody');
    events.forEach((e,i)=>{
      const tr=el('tr');tr.className=i===at?'current':i>at?'future':'';
      tr.append(el('td',e.time));
      for(const c of clients){
        const td=el('td');
        if(c===e.client){td.append(el('b',e.step),el('span',' '+e.op,'op'));if(e.note!=='-')td.append(el('span',' → '+e.note,'note'))}
        tr.append(td);
      }
      tr.append(el('td',e.index,'mono'));body.append(tr);
    });
    table.append(body);
    $('lane-position').textContent=`Step ${at+1} of ${events.length}`;
    $('lane-back').disabled=at===0;$('lane-reset').disabled=at===0;$('lane-step').disabled=at===events.length-1;$('lane-end').disabled=at===events.length-1;
    body.children[at]?.scrollIntoView({block:'nearest'});
  };
  const go=i=>{lane.at=Math.max(0,Math.min(lane.events.length-1,i));renderLanes()};
  $('lane-reset').onclick=()=>go(0);$('lane-back').onclick=()=>go(lane.at-1);$('lane-step').onclick=()=>go(lane.at+1);$('lane-end').onclick=()=>go(lane.events.length-1);

  const replay=()=>{
    const {code,out}=engine.call('replay\n'+$('race-file').value);
    const verdict=$('lane-verdict');verdict.className='lane-verdict';
    if(code===2){verdict.textContent='Cannot replay: '+out.replace(/^error: /,'').trim();verdict.classList.add('fail');lane={events:[],at:0};$('lanes').replaceChildren();$('race-history').replaceChildren();return}
    const n=parseNarration(out);
    verdict.textContent=(code===1?'× ':'✓ ')+n.verdict;verdict.classList.add(code===1?'fail':'pass');
    lane={events:n.events,at:n.events.length-1};renderLanes();
    const h=$('race-history');h.replaceChildren();
    const head=el('tr');head.append(...['client','operation','invoked','responded','answer'].map(x=>el('th',x)));
    h.append(el('thead'));h.firstChild.append(head);
    const body=el('tbody');for(const r of n.history){const tr=el('tr');tr.append(el('td',r.client),el('td',r.op,'mono'),el('td',r.invoked),el('td',r.responded),el('td',r.answer,'mono'));body.append(tr)}
    h.append(body);
  };
  $('race-replay').onclick=()=>busy($('race-replay'),replay);
  $('race-run').onclick=()=>busy($('race-run'),()=>{
    try{
      const v=$('race-variant').value,seed=int('race-seed',0,2**53),runs=int('race-runs',1,5000);
      const {code,out}=engine.call(`race ${v} ${seed} ${runs}`);
      if(code===2)throw Error(out.replace(/^error: /,''));
      const [report,file]=out.split(SEP);
      $('race-report').textContent=summary(report,'race');$('race-file').value=file;replay();
    }catch(err){$('race-report').textContent=err.message}
  });
}
