// The flight recorder: missions, HUD and input around the canvas scene. All verdicts come
// from the Rust engine (WebAssembly) or the packaged engine traces; nothing is recomputed here.
import {parse} from './replay.mjs';
import {load,SEP} from './engine.mjs';
import {traceFrames,raceFrames,domainOf,clock} from './frames.mjs';
import {Scene} from './scene.mjs';
import {Sound,prefersReducedMotion} from './fx.mjs';

const $=id=>document.getElementById(id);
const scene=new Scene($('world'));
const sound=new Sound();
const store={get(k){try{return localStorage.getItem('pulsefold.'+k)}catch{return null}},set(k,v){try{localStorage.setItem('pulsefold.'+k,v)}catch{}}};

const S={mission:null,frames:[],at:0,timer:null,engine:null,busy:false,
  ghost:'fault',traces:{},hunt:{variant:'stale-index'},race:{variant:'torn-move',file:'',locked:false,result:null},
  reduced:prefersReducedMotion()||store.get('still')==='1'};

const BUGS=[
  ['stale-index','bug','A rejected move still writes the attempted slot into the index.'],
  ['leftover-entry','bug','A valid move adds the new index entry but leaves the old one.'],
  ['closed-interval','bug','Index lookups treat touching intervals as overlapping.'],
  ['wrong-bucket','bug','Every booking is filed under the first chamber.'],
  ['index-before-check','bug','A reserve writes the index before checking it may.'],
  ['no-self-exclusion','bug','A move conflicts with its own current slot.'],
  ['indexed','control','The correct room index. Nothing should be found.'],
];
const RACES=[
  ['torn-move','bug','A move unindexes, then re-indexes, while reads take no lock.'],
  ['check-then-act','bug','Writers check, then act, with no lock in between.'],
  ['locked-writes','control','Writers lock; a move re-indexes in one step.'],
  ['locked','control','Everything holds the lock.'],
];

/* ---------- Motion and sound preferences ---------- */
function setStill(on){
  S.reduced=on;store.set('still',on?'1':'0');scene.setReduced(on);
  document.body.classList.toggle('still',on);$('motion').setAttribute('aria-pressed',String(!on));
}
setStill(S.reduced);
$('motion').onclick=()=>setStill(!S.reduced);
$('sound').onclick=()=>{const on=!sound.on;sound.enable(on);$('sound').setAttribute('aria-pressed',String(on));if(on)sound.pass()};

/* ---------- Engine ---------- */
const engineReady=load('pulsefold.wasm').then(e=>{S.engine=e;$('engine-chip').textContent='RUST · WASM';$('engine-chip').className='chip live';return e})
  .catch(err=>{$('engine-chip').textContent='ENGINE OFFLINE';$('engine-chip').className='chip coral';$('engine-chip').title=err.message;
    for(const b of document.querySelectorAll('[data-mission=hunt],[data-mission=race]'))b.disabled=true;});

/* ---------- Banner ---------- */
let bannerTimer=0;
function banner(title,sub,tone='coral'){
  const b=$('banner');b.className='banner '+tone;b.querySelector('b').textContent=title;b.querySelector('span').textContent=sub??'';
  void b.offsetWidth;b.classList.add(S.reduced?'static':'show');
  clearTimeout(bannerTimer);bannerTimer=setTimeout(()=>{b.className='banner'},2300);
}

/* ---------- Screens ---------- */
function showOnly(...ids){
  for(const id of ['title','setup','scan','cards','logbox','caption','transport'])$(id).hidden=!ids.includes(id);
}
function toTitle(){
  stop();S.mission=null;S.frames=[];showOnly('title');scene.setMode('title');scene.scan(false);
  $('mission-name').textContent='Flight recorder';$('mission-chip').textContent='ORBITAL METABOLISM';$('mission-chip').className='chip';$('meta').textContent='';
  document.querySelector('.mission-card')?.focus({preventScroll:true});
}
for(const b of document.querySelectorAll('[data-mission]'))b.onclick=()=>startMission(b.dataset.mission);
$('home').onclick=toTitle;$('setup-back').onclick=toTitle;

function startMission(m){
  if(S.busy)return;
  if(m==='ghost')return startGhost();
  if(m==='hunt')return setup('hunt');
  if(m==='race')return setup('race');
}

function setup(kind){
  stop();S.mission=kind;showOnly('setup');scene.setMode('title');
  const hunt=kind==='hunt',list=hunt?BUGS:RACES,state=hunt?S.hunt:S.race;
  $('mission-name').textContent=hunt?'Seed Hunt':'Race Condition';$('mission-chip').textContent='LIVE ENGINE';$('mission-chip').className='chip live';
  $('setup-eyebrow').textContent=hunt?'MISSION 02 · SEEDED SIMULATION':'MISSION 03 · CONCURRENT CLIENTS';
  $('setup-title').textContent=hunt?'Plant a bug. Let the seeds find it.':'Two clients. One lock. Find the race.';
  $('setup-lede').textContent=hunt
    ?'The engine generates operations from each seed, runs them against your pick and a separate reference, and stops at the first disagreement. Then it shrinks the failure to its smallest form and replays it.'
    :'A seeded scheduler interleaves clients one micro-step at a time. Every history must be linearizable: some real-time order of the operations has to reproduce every answer and the final state.';
  const box=$('setup-options');box.replaceChildren();
  for(const [name,kind2,text] of list){
    const o=document.createElement('button');o.className='option';o.setAttribute('role','radio');o.setAttribute('aria-checked',String(name===state.variant));
    const em=document.createElement('em');em.textContent=kind2==='bug'?'PLANTED BUG':'CORRECT CONTROL';em.className=kind2;
    const b=document.createElement('b');b.textContent=name;const s=document.createElement('span');s.textContent=text;
    o.append(em,b,s);
    o.onclick=()=>{state.variant=name;for(const x of box.children)x.setAttribute('aria-checked',String(x===o))};
    box.append(o);
  }
  $('launch').textContent=hunt?'Launch hunt':'Search for a race';
  $('launch').onclick=()=>hunt?launchHunt():launchRace();
  $('launch').focus({preventScroll:true});
}

/* ---------- Mission 01: the scripted Ghost Cycle ---------- */
async function startGhost(){
  stop();S.mission='ghost';
  $('mission-name').textContent='The Ghost Cycle';$('mission-chip').textContent='SCRIPTED SESSION';$('mission-chip').className='chip';
  try{
    for(const k of ['mutant','control'])if(!S.traces[k])S.traces[k]=parse(await (await fetch(k+'.tsv')).text());
  }catch(err){banner('REPLAY MISSING',err.message);return toTitle()}
  loadGhost(S.ghost);
}
function loadGhost(which){
  S.ghost=which;
  const frames=traceFrames(S.traces[which==='fault'?'mutant':'control']);
  play(frames,{extra:()=>{
    const g=document.createElement('div');g.className='seg-group';g.setAttribute('role','group');g.setAttribute('aria-label','Session');
    for(const [k,label] of [['fault','Stale index'],['control','Control']]){
      const b=document.createElement('button');b.className='seg';b.textContent=label;b.setAttribute('aria-pressed',String(S.ghost===k));
      b.onclick=()=>{if(S.ghost!==k)loadGhost(k)};g.append(b);
    }
    return [g];
  },meta:()=>which==='fault'?'turn 3 ships the planted bug':'turn 3 ships the correct index'});
}

/* ---------- Mission 02: Seed Hunt ---------- */
const nextFrame=()=>new Promise(r=>requestAnimationFrame(()=>r()));
const wait=ms=>new Promise(r=>setTimeout(r,S.reduced?Math.min(ms,120):ms));
const fields=text=>Object.fromEntries(text.trim().split('\n').map(l=>{const i=l.indexOf('\t');return [l.slice(0,i),l.slice(i+1)]}));

async function search(kind,variant,limit,chunk,request){
  S.busy=true;showOnly('scan');scene.setMode('title');scene.scan(true);
  $('scan-label').textContent=kind==='hunt'?`HUNTING ${variant.toUpperCase()}`:`SCHEDULING ${variant.toUpperCase()}`;
  let seed=1,found=null;
  try{
    while(seed<=limit){
      const {code,out}=S.engine.call(request(seed,chunk));
      if(code===2)throw Error(out);
      const [report,payload]=out.split(SEP),f=fields(report);
      const shown=Math.min(limit,seed+chunk-1);$('scan-count').textContent=shown.toLocaleString();
      $('scan-sub').textContent=`seeds tested · ${kind==='hunt'?'48 operations each':'2–3 clients each'}`;
      sound.tick();
      if(f.first_failure&&f.first_failure!=='-'){found={report:f,payload,seed:+f.first_failure.match(/seed (\d+)/)[1]};break}
      if(seed+chunk>limit){found={report:f,payload,seed:null,clean:true,tested:limit};break}
      seed+=chunk;await nextFrame();
    }
  }finally{scene.scan(false);S.busy=false}
  return found;
}

async function launchHunt(){
  if(!S.engine||S.busy)return;
  const v=S.hunt.variant;
  const r=await search('hunt',v,5000,50,(seed,n)=>`sim ${v} ${seed} 48 ${n}`);
  if(!r)return;
  $('scan-count').textContent='';
  if(r.clean){
    banner('CLEAN SWEEP',`${r.tested.toLocaleString()} SEEDS · NO DISAGREEMENT`,'mint');sound.pass();await wait(900);
    // Show the first seed's run so there is still something to inspect.
    const {out}=S.engine.call(`sim ${v} 1 48 1`);
    return play(traceFrames(parse(out.split(SEP)[1])),{huntMeta:`seed 1 · ${v} · no failure`});
  }
  sound.found();banner('CONTAMINATION DETECTED',`SEED ${r.seed} · ${r.report.reason}`.toUpperCase().slice(0,90));
  const before=+r.report.first_failure.match(/operation (\d+)/)[1],after=parseInt(r.report.shrunk,10);
  $('scan-label').textContent='SHRINKING';$('scan-sub').textContent='operations in the smallest failing case';
  $('scan').hidden=false;
  const steps=S.reduced?1:18;
  for(let i=0;i<=steps;i++){$('scan-count').textContent=String(Math.round(before+(after-before)*(i/steps)**.6));sound.tick();await wait(45)}
  await wait(500);
  const frames=traceFrames(parse(r.payload));
  play(frames,{huntMeta:`seed ${r.seed} · ${v} · ${before} → ${after} ops`,autoplay:true});
}

/* ---------- Mission 03: Race Condition ---------- */
async function launchRace(){
  if(!S.engine||S.busy)return;
  const v=S.race.variant;
  const r=await search('race',v,2000,50,(seed,n)=>`race ${v} ${seed} ${n}`);
  if(!r)return;
  S.race.locked=false;
  if(r.clean){banner('LINEARIZABLE',`${r.tested.toLocaleString()} SEEDS · EVERY HISTORY HOLDS`,'mint');sound.pass();await wait(700)}
  else{
    sound.found();banner('RACE FOUND',`SEED ${r.seed} · NOT LINEARIZABLE`);
    $('scan-label').textContent='SHRINKING';$('scan-sub').textContent='micro-steps in the smallest failing interleaving';$('scan').hidden=false;
    await wait(900);
  }
  S.race.file=r.payload;S.race.seed=r.seed;
  replayRace(r.payload,{autoplay:true});
}

function replayRace(file,{autoplay=false}={}){
  const {code,out}=S.engine.call('replay\n'+file);
  if(code===2){banner('CANNOT REPLAY',out.replace(/^error: /,'').trim());return}
  const result=raceFrames(out);S.race.result=result;
  const variant=result.variant;
  play(result.frames,{autoplay,raceMeta:`${S.race.seed?'seed '+S.race.seed+' · ':''}${variant}`,
    extra:()=>{
      const b=document.createElement('button');
      const original=S.race.file.match(/^variant (.*)$/m)[1];
      const canLock=original!=='locked';
      b.textContent=S.race.locked?`Remove lock (${original})`:'Apply lock';b.disabled=!canLock;
      b.className=S.race.locked?'':'primary';
      b.title=canLock?'Replay the same interleaving with every operation holding the lock':'Already locked';
      b.onclick=()=>{S.race.locked=!S.race.locked;const f=S.race.locked?S.race.file.replace(/^variant .*$/m,'variant locked'):S.race.file;replayRace(f,{autoplay:true})};
      const again=document.createElement('button');again.textContent='New search';again.className='ghost';again.onclick=()=>setup('race');
      return [b,again];
    }});
}

/* ---------- Playback ---------- */
let playOpts={};
function play(frames,opts={}){
  stop();S.frames=frames;S.at=0;playOpts=opts;
  showOnly('cards','logbox','caption','transport');
  scene.setMode(S.mission);scene.setDomain(domainOf(frames));
  buildScrub();
  const extra=$('extra');extra.replaceChildren(...(opts.extra?opts.extra():[]));
  if(S.mission==='hunt'){const b=document.createElement('button');b.textContent='New hunt';b.className='ghost';b.onclick=()=>setup('hunt');extra.append(b)}
  goto(0,{cue:false});
  if(opts.autoplay)start();
  $('t-play').focus({preventScroll:true});
}

function buildScrub(){
  const box=$('scrub');box.replaceChildren();
  S.frames.forEach((f,i)=>{
    const b=document.createElement('button');
    b.setAttribute('aria-label',`Step ${i+1}: ${f.kind==='op'?f.title:f.caption}`);
    if(f.kind==='op'&&f.verdict==='FAIL')b.classList.add('fail');
    if(f.kind==='micro'&&f.micro==='read'&&f.ghosts.length+f.missing.length)b.classList.add('fail');
    if(i>0&&f.kind==='op'&&f.turn!==S.frames[i-1].turn)b.classList.add('turn');
    b.onclick=()=>{stop();goto(i)};box.append(b);
  });
}

function firstFailureIndex(){
  return S.frames.findIndex(f=>f.kind==='op'?f.verdict==='FAIL':f.micro==='read'&&f.ghosts.length+f.missing.length>0);
}

function goto(i,{cue=true}={}){
  if(!S.frames.length)return;
  const prev=S.at;S.at=Math.max(0,Math.min(S.frames.length-1,i));
  const f=S.frames[S.at];
  scene.show(f,{cue:cue&&S.at!==prev});
  hud(f);
  if(cue&&S.at!==prev){
    if(f.kind==='op'){if(f.verdict==='FAIL'){sound.fail();flash('card-a')}else if(f.result==='CONFLICT')sound.reject();else sound.step()}
    else{
      const torn=f.micro==='read'&&f.ghosts.length+f.missing.length>0;
      if(torn){sound.fail();flash('card-c');if(S.timer){stop();banner('TORN READ','THE READ SAW AN INDEX THAT WAS NEVER TRUE')}}
      else sound.step();
      if(S.at===S.frames.length-1)finishRace();
    }
  }
}

function finishRace(){
  const r=S.race.result;if(!r)return;
  if(r.failed){banner('NOT LINEARIZABLE','NO REAL-TIME ORDER REPRODUCES THESE ANSWERS');sound.fail()}
  else{banner(S.race.locked?'THE LOCK HOLDS':'LINEARIZABLE',S.race.locked?'SAME INTERLEAVING · EVERY ANSWER EXPLAINED':'EVERY ANSWER HAS A REAL-TIME ORDER','mint');scene.wave('#8de7b5');sound.fixed()}
}

function flash(id){const c=$(id);c.classList.remove('flash');void c.offsetWidth;c.classList.add('flash')}

function card(id,k,v,s,tone){const c=$(id);c.className='card glass '+tone;c.querySelector('.k').textContent=k;c.querySelector('.v').textContent=v;c.querySelector('.s').textContent=s}

function hud(f){
  const n=S.frames.length;
  $('caption').textContent=f.caption;
  $('t-pos').textContent=f.kind==='op'?`turn ${f.turn} · step ${S.at+1}/${n}`:`t=${f.time} · step ${S.at+1}/${n}`;
  $('meta').textContent=playOpts.huntMeta??playOpts.raceMeta??(playOpts.meta?playOpts.meta():'');
  if(f.kind==='op'){
    const bad=f.verdict==='FAIL';
    card('card-a','BEHAVIORAL CHECK',bad?'× Invariant failed':'✓ No violation',bad?f.reason:'Matches the reference at this step.',bad?'coral':'mint');
    const exact=f.marker==='EXACT';
    card('card-b','CANARY',(exact?'◇ ':'? ')+f.marker.toLowerCase(),S.mission==='ghost'?`Scored from turn ${f.turn}’s scripted response.`:'A simulation has no agent response.',exact?'mint':'lilac');
    card('card-c','REFACTOR · REVIEWER',f.refactor==='n/a'?'Not asked':f.refactor.replace('_',' '),`Turn ${f.turn} ships ${f.variant}.`,'cyan');
    card('card-d','OPERATION',f.result==='OK'?'Accepted':f.result.replace('_',' '),f.title,f.result==='OK'?'cyan':'amber');
  }else{
    const r=S.race.result,torn=f.ghosts.length+f.missing.length>0;
    card('card-a','HISTORY',r.failed?'× Not linearizable':'✓ Linearizable',r.failed?'No real-time order reproduces every answer.':'Some real-time order explains every answer.',r.failed?'coral':'mint');
    card('card-b','LOCK',f.lock?(f.lock==='setup'?'Held · setup':`Held · client ${f.lock}`):'Free',r.variant==='check-then-act'?'Writers take no lock in this build.':'Readers lock only in the locked build.',f.lock?'cyan':'mist');
    card('card-c','INDEX vs LEDGER',torn?'≠ Diverged':'= In sync',torn?`${f.missing.map(s=>s.id+' unindexed').concat(f.ghosts.map(s=>'ghost '+s.id)).join(', ')}`:'Every booking has its index entry.',torn?'coral':'mint');
    card('card-d','STEP',`${f.client==='setup'?'Setup':'Client '+f.client} · ${f.micro}`,f.op,'cyan');
  }
  // Scrubber and log.
  [...$('scrub').children].forEach((b,i)=>{b.classList.toggle('now',i===S.at);b.classList.toggle('done',i<S.at)});
  const log=$('log');log.replaceChildren();
  S.frames.forEach((g,i)=>{
    const li=document.createElement('li');const num=document.createElement('i');num.textContent=String(i+1).padStart(2,'0');
    li.append(num,document.createTextNode(g.kind==='op'?`${g.op} → ${g.result}`:`${g.client==='setup'?'S':g.client} ${g.micro}${g.note!=='-'?' '+g.note:''}`));
    if(i===S.at)li.className='now';else if(i>S.at)li.className='future';
    if((g.kind==='op'&&g.verdict==='FAIL')||(g.kind==='micro'&&g.micro==='read'&&g.ghosts.length+g.missing.length))li.classList.add('fail');
    li.onclick=()=>{stop();goto(i)};log.append(li);
  });
  log.querySelector('.now')?.scrollIntoView({block:'nearest'});
  $('t-back').disabled=$('t-rewind').disabled=S.at===0;$('t-step').disabled=S.at===n-1;
  const ff=firstFailureIndex();$('t-fail').disabled=ff<0;$('t-fail').textContent=ff<0?'No failure':'First failure';
}

/* ---------- Transport ---------- */
function stop(){if(S.timer){clearInterval(S.timer);S.timer=null}$('t-play').textContent='▶';$('t-play').setAttribute('aria-label','Play')}
function start(){
  if(!S.frames.length)return;
  if(S.at>=S.frames.length-1)goto(0,{cue:false});
  $('t-play').textContent='❚❚';$('t-play').setAttribute('aria-label','Pause');
  const ms=S.frames[0].kind==='micro'?520:850;
  S.timer=setInterval(()=>{goto(S.at+1);if(S.at>=S.frames.length-1)stop()},ms);
}
$('t-play').onclick=()=>S.timer?stop():start();
$('t-step').onclick=()=>{stop();goto(S.at+1)};
$('t-back').onclick=()=>{stop();goto(S.at-1)};
$('t-rewind').onclick=()=>{stop();goto(0)};
$('t-fail').onclick=()=>{stop();const i=firstFailureIndex();if(i>=0)goto(i)};

addEventListener('keydown',e=>{
  const el=e.target instanceof Element?e.target:null;
  if(el?.closest('input,textarea,select'))return;
  const playing=!$('transport').hidden;
  if(e.key==='Escape'){toTitle();return}
  if(e.key==='m'||e.key==='M'){$('sound').click();return}
  if(!$('title').hidden&&['1','2','3'].includes(e.key)){startMission(['ghost','hunt','race'][+e.key-1]);return}
  if(!playing)return;
  if(e.key==='ArrowRight'){e.preventDefault();$('t-step').click()}
  else if(e.key==='ArrowLeft'){e.preventDefault();$('t-back').click()}
  else if(e.key===' '&&!el?.closest('button')){e.preventDefault();$('t-play').click()}
  else if(e.key==='Home'){$('t-rewind').click()}
  else if(e.key==='f'||e.key==='F'){$('t-fail').click()}
});

toTitle();
engineReady.then(()=>{});
export {S,clock};
