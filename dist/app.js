import {parse,firstFailure,seek,domain} from './replay.mjs';
const $=id=>document.getElementById(id);
const pieces=new Map();
let mode='fault', at=0, timer=null, range=[600,840];
const time=x=>Math.floor(x/60)+':'+String(x%60).padStart(2,'0');
const same=(a,b)=>a&&b&&a.id===b.id&&a.room===b.room&&a.lo===b.lo&&a.hi===b.hi;
const colors={cyan:'#8cd8f8',ink:'#edf5f7',muted:'#adc2ca',coral:'#ff928a',track:'#15212a',line:'#486174',lift:'#2d4050'};
function stop(){if(timer!==null) clearInterval(timer);timer=null;$('play').textContent='Play'}
function move(i){if(!pieces.has(mode))return;at=seek(pieces.get(mode).rows,i);render()}
function modeSet(k){if(!pieces.has(k))return;stop();mode=k;at=0;range=domain(pieces.get(mode).rows);render()}
function traceTitle(row){
  const [op,id]=row.op.split(' ');
  return op==='reserve'?'Reserve '+id:op==='move'?(row.result==='CONFLICT'?'Reject move':'Move '+id):row.op;
}
function graph(data,other,idx){
  const x=t=>52+(t-range[0])/(range[1]-range[0])*390;
  const y=r=>r===1?41:90;
  const ticks=Array.from({length:5},(_,i)=>Math.round(range[0]+i*(range[1]-range[0])/4));
  const h='hatch'+idx;
  const desc=data.map(z=>'Cycle '+z.id+' in chamber C'+z.room+' from '+time(z.lo)+' to '+time(z.hi)).join('. ');
  const missing=idx?other.filter(z=>!data.some(k=>same(k,z))):[];
  const ghost=idx?data.filter(z=>!other.some(k=>same(k,z))):[];
  return `<svg class="graph" viewBox="0 0 472 138" role="img" aria-label="${desc||'No reservations'}">
    <defs><pattern id="${h}" width="7" height="7" patternUnits="userSpaceOnUse" patternTransform="rotate(45)"><rect width="7" height="7" fill="${colors.coral}"/><path d="M0 0V7" stroke="${colors.track}" stroke-width="2"/></pattern></defs>
    ${[1,2].map(r=>`<rect x="45" y="${y(r)-5}" width="418" height="38" rx="5" fill="${colors.track}"/><text x="10" y="${y(r)+18}" fill="${colors.muted}" font-size="13">C${r}</text>`).join('')}
    ${ticks.map(t=>`<text x="${x(t)}" y="19" text-anchor="middle" fill="${colors.muted}" font-size="12">${time(t)}</text><path d="M${x(t)} 27V128" stroke="${colors.line}" stroke-width=".7"/>`).join('')}
    ${missing.map(z=>`<rect x="${x(z.lo)}" y="${y(z.room)}" width="${Math.max(1,x(z.hi)-x(z.lo))}" height="24" rx="3" fill="none" stroke="${colors.muted}" stroke-dasharray="4 3"/>`).join('')}
    ${[...data].sort((a,b)=>Number(ghost.includes(a))-Number(ghost.includes(b))).map(z=>{const g=ghost.includes(z), wide=x(z.hi)-x(z.lo), base=y(z.room)+(g?25:0);return `<rect x="${x(z.lo)}" y="${base}" width="${Math.max(1,wide)}" height="${g?6:24}" rx="3" fill="${g?'url(#'+h+')':(z.id==='A'?colors.cyan:colors.lift)}"/>${wide>25?`<text x="${x(z.lo)+5}" y="${base+(g?15:17)}" fill="${g?colors.coral:z.id==='A'?colors.track:colors.ink}" font-size="12">${g?'GHOST ':''}${z.id.slice(0,4)}</text>`:''}`;}).join('')}
  </svg>`;
}
function panel(row,idx){
  const diffs=row.oldL!==row.oldI||row.newL!==row.newI||row.ledger.length!==row.index.length||row.ledger.some(a=>!row.index.some(b=>same(a,b)));
  const side=idx?row.index:row.ledger, other=idx?row.ledger:row.index;
  const val=(k,l,label)=>`<span class="${idx&&k!==l?'different':''}">${label} <strong>${k.replaceAll(',', ' + ')||'None'}</strong></span>`;
  return `<article class="instrument"><header><h2>${idx?'Chamber index':'Canonical ledger'}</h2><span class="badge ${idx&&diffs?'fail':''}">${idx?(diffs?'≠ DIVERGED':'= IN SYNC'):'REFERENCE'}</span></header>${graph(side,other,idx)}<div class="answers">${val(idx?row.oldI:row.oldL,row.oldL,'10:15 window:')}${val(idx?row.newI:row.newL,row.newL,'13:00 window:')}</div></article>`;
}
function render(){
  const set=pieces.get(mode);if(!set)return;
  const row=set.rows[at],first=firstFailure(set.rows),bad=row.verdict==='FAIL';
  $('fault').setAttribute('aria-pressed',mode==='fault');$('control').setAttribute('aria-pressed',mode==='control');
  $('notice').textContent=(mode==='import'?'IMPORTED TRACE · VERDICTS AS RECORDED':'SYNTHETIC CONTROL TRACE · ILLUSTRATIVE, NOT LIVE DATA').replace('CONTROL TRACE', mode==='fault'?'FAULT TRACE':'CONTROL TRACE');
  $('verdict-card').classList.toggle('fail',bad);$('verdict').textContent=bad?'× Invariant failed':'✓ No violation';$('reason').textContent=bad?'Recorded behavioral check failed.':'Recorded checks passed at this step.';
  $('marker').textContent=(row.marker==='EXACT'?'◇ ':'? ')+row.marker.toLowerCase(); $('marker').parentElement.classList.toggle('mint',row.marker==='EXACT'); $('marker').nextElementSibling.textContent=mode==='import'?'Imported marker status.':'Scripted terminal marker only.';
  $('result-card').classList.toggle('reject',row.result!=='OK');$('result').textContent=row.result==='OK'?'Accepted':row.result;$('op').textContent=traceTitle(row);
  $('position').textContent='Step '+row.n+' of '+set.rows.length;
  $('panels').innerHTML=panel(row,0)+panel(row,1);
  if($('timeline').children.length !== set.rows.length || $('timeline').dataset.mode !== mode || mode==='import'&&$('timeline').dataset.revision!==String(importRevision)){
    $('timeline').dataset.mode=mode;$('timeline').dataset.revision=String(importRevision);
    $('timeline').replaceChildren(...set.rows.map((r,i)=>{
      const b=document.createElement('button'), num=document.createElement('span');
      num.textContent=String(r.n).padStart(2,'0');b.append(num,document.createTextNode(traceTitle(r)));
      b.dataset.fail=String(r.verdict==='FAIL');b.onclick=()=>{stop();move(i)};return b;
    }));
  }
  [...$('timeline').children].forEach((b,i)=>b.setAttribute('aria-current',i===at?'step':'false'));
  $('back').disabled=at===0;$('reset').disabled=at===0;$('step').disabled=at===set.rows.length-1;
  $('jump').disabled=first<0;$('jump').textContent=first<0?'No failing step':'First failure';
}
let importRevision=0;
$('fault').onclick=()=>modeSet('fault');$('control').onclick=()=>modeSet('control');
$('back').onclick=()=>{stop();move(at-1)};$('step').onclick=()=>{stop();move(at+1)};$('reset').onclick=()=>{stop();move(0)};
$('jump').onclick=()=>{if(pieces.has(mode)){stop();move(firstFailure(pieces.get(mode).rows))}};
$('play').onclick=()=>{
  if(!pieces.has(mode))return;
  if(timer!==null){stop();return}
  const end=pieces.get(mode).rows.length-1;if(at>=end)move(0);
  $('play').textContent='Pause';
  timer=setInterval(()=>{move(at+1);if(at>=end)stop()},800);
};
$('export').onclick=()=>{
  if(!pieces.has(mode))return;
  const uri=URL.createObjectURL(new Blob([pieces.get(mode).raw],{type:'text/tab-separated-values'}));
  const a=document.createElement('a');a.href=uri;a.download='pulsefold-'+mode+'.tsv';a.click();setTimeout(()=>URL.revokeObjectURL(uri),1000);
};
$('import').onchange=async e=>{
  const f=e.target.files[0];if(!f)return;
  try{
    if(f.size>1048576)throw Error('Please choose a replay smaller than 1 MB.');
    const raw=await f.text(),rows=parse(raw);
    pieces.set('import',{raw,rows});importRevision++;modeSet('import');
  }catch(err){$('notice').textContent=err.message}
  finally{e.target.value=''}
};
try{
  const raw=await Promise.all(['mutant.tsv','control.tsv'].map(async name=>{const res=await fetch(name);if(!res.ok)throw Error('Replay not found');return res.text()}));
  ['fault','control'].forEach((k,i)=>pieces.set(k,{raw:raw[i],rows:parse(raw[i])}));modeSet('fault');
}catch(err){$('notice').textContent='Could not load replay: '+err.message}
