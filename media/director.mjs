// Records a full walkthrough of the flight recorder as a 1080p60 video, deterministically:
// the page runs on a virtual clock (clock.js) and every frame is a lossless screenshot, so
// the result is a perfect 60 fps regardless of machine load. Because the clock is virtual,
// any Chrome instance can fast-forward to any frame, so the capture is split across workers.
//
//   node media/director.mjs              # full take -> media/out/pulsefold-walkthrough-1080p60.mp4
//   node media/director.mjs --smoke      # a few seconds in one process, to check the pipeline
//   node media/director.mjs --workers 6  # choose the parallelism (default 8)
//
// Internal modes: --plan (count frames and chapters), --worker START END FILE (capture a slice).
// Needs google-chrome, ffmpeg with libx264 and python3 (static server). No npm packages:
// Chrome is driven over the DevTools protocol with Node's built-in WebSocket.
import {spawn} from 'node:child_process';
import {mkdirSync,readFileSync,writeFileSync,rmSync,mkdtempSync,renameSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join,dirname} from 'node:path';
import {fileURLToPath} from 'node:url';

const HERE=dirname(fileURLToPath(import.meta.url)),ROOT=join(HERE,'..'),OUT=join(HERE,'out');
const argv=process.argv.slice(2);
const SMOKE=argv.includes('--smoke'),PLAN=argv.includes('--plan'),WORKER=argv.indexOf('--worker');
const W=1920,H=1080,FPS=60,DT=1000/FPS;
const slot=WORKER>=0?+argv[WORKER+4]:PLAN?99:0;
const PORT=8800+slot,DBG=9400+slot;
const RANGE=WORKER>=0?[+argv[WORKER+1],+argv[WORKER+2]]:[0,Infinity];
const BODY=WORKER>=0?argv[WORKER+3]:join(OUT,'smoke-body.mp4');
const FINAL=join(OUT,SMOKE?'smoke-1080p60.mp4':'pulsefold-walkthrough-1080p60.mp4');
const CAPTURE=!PLAN;
mkdirSync(OUT,{recursive:true});
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
const procs=[];
const run=(cmd,args,opts={})=>{const p=spawn(cmd,args,{stdio:['pipe','ignore','pipe'],...opts});procs.push(p);return p};
const cleanup=()=>{for(const p of procs.reverse())try{p.kill('SIGTERM')}catch{}};
process.on('exit',cleanup);process.on('SIGINT',()=>{cleanup();process.exit(130)});
const runNode=(args,label)=>new Promise((ok,bad)=>{
  const p=spawn(process.execPath,[fileURLToPath(import.meta.url),...args],{stdio:['ignore','pipe','pipe']});procs.push(p);
  let out='';p.stdout.on('data',d=>out+=d);p.stderr.on('data',d=>process.stderr.write(`[${label}] ${d}`));
  p.on('exit',c=>c===0?ok(out):bad(Error(`${label} exited ${c}`)));
});
const ffmpeg=args=>new Promise((ok,bad)=>{const p=spawn('ffmpeg',['-y','-loglevel','error',...args],{stdio:'inherit'});p.on('exit',c=>c===0?ok():bad(Error('ffmpeg exited '+c)))});

if(!SMOKE&&!PLAN&&WORKER<0){
  // Coordinator: plan the timeline, capture slices in parallel, join them losslessly.
  const started=Date.now();
  const wi=argv.indexOf('--workers'),K=wi>=0?Math.max(1,+argv[wi+1]):8;
  const plan=JSON.parse(await runNode(['--plan'],'plan'));
  console.error(`plan: ${plan.frames} frames (${(plan.frames/FPS).toFixed(1)} s), ${K} workers`);
  const per=Math.ceil(plan.frames/K),segs=[];
  await Promise.all(Array.from({length:K},(_,k)=>{
    const a=k*per,b=Math.min(plan.frames,(k+1)*per);if(a>=b)return null;
    const seg=join(OUT,`segment-${String(k).padStart(2,'0')}.mp4`);segs[k]=seg;
    return runNode(['--worker',String(a),String(b),seg,String(k+1)],`worker ${k+1}`);
  }));
  const list=join(OUT,'segments.txt');writeFileSync(list,segs.filter(Boolean).map(f=>`file '${f}'`).join('\n')+'\n');
  const total=Math.round(plan.frames*DT);
  const meta=[';FFMETADATA1','title=Pulsefold: flight recorder walkthrough',...plan.chapters.flatMap((c,i)=>['[CHAPTER]','TIMEBASE=1/1000',`START=${c.at}`,`END=${plan.chapters[i+1]?.at??total}`,`title=${c.title}`])].join('\n');
  const metaFile=join(OUT,'chapters.txt');writeFileSync(metaFile,meta+'\n');
  await ffmpeg(['-f','concat','-safe','0','-i',list,'-i',metaFile,'-map','0','-map_metadata','1','-map_chapters','1','-c','copy','-movflags','+faststart',FINAL]);
  for(const f of [...segs.filter(Boolean),list])rmSync(f,{force:true});
  const fmt=ms=>`${String(Math.floor(ms/60000)).padStart(2,'0')}:${String(Math.floor(ms/1000)%60).padStart(2,'0')}`;
  console.log(JSON.stringify({video:FINAL,frames:plan.frames,seconds:+(total/1000).toFixed(2),wallSeconds:Math.round((Date.now()-started)/1000),
    chapters:plan.chapters.map(c=>`${fmt(c.at)} ${c.title}`)},null,1));
  cleanup();process.exit(0);
}

async function waitFor(fn,what,ms=20000){const t=Date.now();for(;;){try{const v=await fn();if(v)return v}catch{}if(Date.now()-t>ms)throw Error('timed out waiting for '+what);await sleep(100)}}

// --- Environment: a static server and headless Chrome at 1920x1080 ---
run('python3',['-m','http.server',String(PORT),'--bind','127.0.0.1','--directory',join(ROOT,'dist')]);
const profile=mkdtempSync(join(tmpdir(),'pulsefold-chrome-'));
run('google-chrome',['--headless=new',`--user-data-dir=${profile}`,`--remote-debugging-port=${DBG}`,'--no-first-run','--no-default-browser-check',
  '--hide-scrollbars','--force-device-scale-factor=1',`--window-size=${W},${H}`,'--mute-audio','about:blank']);
const target=await waitFor(async()=>(await (await fetch(`http://127.0.0.1:${DBG}/json`)).json()).find(t=>t.type==='page'),'chrome');

// --- DevTools protocol ---
const ws=new WebSocket(target.webSocketDebuggerUrl);
await new Promise((ok,bad)=>{ws.onopen=ok;ws.onerror=bad});
let seq=0;const pending=new Map();const logs=[];
ws.onmessage=e=>{const m=JSON.parse(e.data);
  if(m.method==='Runtime.exceptionThrown')logs.push(m.params.exceptionDetails?.exception?.description??m.params.exceptionDetails?.text);
  if(m.id&&pending.has(m.id)){const {ok,bad}=pending.get(m.id);pending.delete(m.id);m.error?bad(Error(m.error.message)):ok(m.result)}};
const cdp=(method,params={})=>new Promise((ok,bad)=>{const id=++seq;pending.set(id,{ok,bad});ws.send(JSON.stringify({id,method,params}))});
const js=async expr=>{const r=await cdp('Runtime.evaluate',{expression:`(async()=>{${expr}})()`,awaitPromise:true,returnByValue:true});
  if(r.exceptionDetails)throw Error(r.exceptionDetails.exception?.description??r.exceptionDetails.text);return r.result.value};
await cdp('Runtime.enable');
await cdp('Emulation.setDeviceMetricsOverride',{width:W,height:H,deviceScaleFactor:1,mobile:false});
await cdp('Page.enable');
await cdp('Page.addScriptToEvaluateOnNewDocument',{source:readFileSync(join(HERE,'clock.js'),'utf8')});
await waitFor(async()=>(await fetch(`http://127.0.0.1:${PORT}/`)).ok,'server');
await cdp('Page.navigate',{url:`http://127.0.0.1:${PORT}/`});

// --- Frames: advance the clock one frame, screenshot, feed the encoder ---
const enc=!CAPTURE?null:run('ffmpeg',['-y','-loglevel','error','-f','image2pipe','-framerate',String(FPS),'-c:v','png','-i','-',
  '-c:v','libx264','-preset','slow','-crf','16','-profile:v','high','-pix_fmt','yuv420p','-r',String(FPS),'-movflags','+faststart',BODY]);
let encErr='';enc?.stderr.on('data',d=>encErr+=d);
let frames=0,recording=false;
class Stop extends Error{}
const tick=()=>js('return await window.__vt.tick('+DT+')');
async function frame(){
  if(recording&&frames>=RANGE[1])throw new Stop();
  await tick();
  if(!recording)return;
  if(CAPTURE&&frames>=RANGE[0]){
    const {data}=await cdp('Page.captureScreenshot',{format:'png',optimizeForSpeed:true});
    const buf=Buffer.from(data,'base64');
    if(!enc.stdin.write(buf))await new Promise(r=>enc.stdin.once('drain',r));
    if((frames-RANGE[0]+1)%600===0)console.error(`${(frames-RANGE[0]+1)/FPS}s of slice captured`);
  }
  frames++;
}
/** Runs page code while the clock advances frame by frame, until the code finishes. */
async function act(code){
  let done=false,err=null;
  js(`const D=window.__dir;${code}`).then(()=>done=true,e=>{err=e;done=true});
  while(!done)await frame();
  if(err)throw err;
}
const hold=ms=>act(`await D.sleep(${ms})`);
const key=k=>act(`dispatchEvent(new KeyboardEvent('keydown',{key:'${k}',bubbles:true}));await D.sleep(50)`);

// Let the page load on the virtual clock, then add the cursor and cards.
await waitFor(()=>js(`return document.querySelector('#engine-chip.live')!==null`),'engine',60000);
await js(readFileSync(join(HERE,'kit.js'),'utf8'));
for(let i=0;i<30;i++)await tick();
recording=true;
const chapters=[];const chapter=title=>chapters.push({title,at:Math.round(frames*DT)});

// --- Storyboard ---
async function smoke(){
  chapter('Smoke');
  await act(`await D.click('[data-mission=ghost]');await D.sleep(500);await D.click('#t-play');await D.sleep(4000);`);
}

async function walkthrough(){
  chapter('Pulsefold');
  await act(`await D.sleep(700);await D.card({eyebrow:'PULSEFOLD · FLIGHT RECORDER',title:'A canary can stay exact<br>while the code <span>breaks.</span>',
    body:'Pulsefold is a playable debugger for one question: does an AI agent\\'s instruction canary tell you anything about whether the code it wrote is correct? Everything here runs on a small, dependency-free Rust engine, compiled to WebAssembly and running live in this tab.',hold:9500});`);
  await act(`await D.hover('[data-mission=ghost]');await D.sleep(500);await D.hover('[data-mission=multiverse]');await D.sleep(700);`);

  chapter('01 The Ghost Cycle');
  await act(`await D.card({eyebrow:'MISSION 01 OF 04 · SCRIPTED SESSION',title:'The Ghost Cycle',
    body:'Three scripted turns of an agent working on an orbital habitat\\'s chamber bookings. Turn 3 adds a room index, with a planted bug.',
    watch:['Bookings are bands on two chamber rings. The index is the bright trace riding on them.','Step 9: the move is rejected and the ledger holds, but the index moves anyway. A coral ghost appears.','The canary stays exact the whole time. Then the control session, where turn 3 is correct.']});`);
  await act(`await D.click('[data-mission=ghost]');await D.sleep(1400);await D.click('#t-play');
    while(document.getElementById('t-play').textContent!=='▶')await D.sleep(100);await D.sleep(1200);
    await D.hover('#card-a');await D.sleep(1600);await D.hover('#card-b');await D.sleep(2200);await D.hover('#log li.now');await D.sleep(1600);
    await D.click('.seg','Control');await D.sleep(900);await D.click('#t-play');
    while(document.getElementById('t-play').textContent!=='▶')await D.sleep(100);await D.sleep(2400);`);

  chapter('02 Seed Hunt');
  await key('Escape');await hold(500);
  await act(`await D.card({eyebrow:'MISSION 02 OF 04 · LIVE ENGINE',title:'Seed Hunt',
    body:'Plant a bug and let seeded simulation find it. The engine generates operations from each seed, runs them against the bug and a separate reference, and stops at the first disagreement.',
    watch:['The seed odometer: the Rust engine searching, in the browser.','Contamination detected, then shrunk from 27 operations to the 3 that matter.','The replay: which probe window disagrees, and what each side answered.']});`);
  await act(`await D.click('[data-mission=hunt]');await D.sleep(1200);await D.hover('.option','stale-index');await D.sleep(900);await D.click('.option','stale-index');await D.sleep(500);await D.click('#launch');
    while(document.getElementById('transport').hidden)await D.sleep(100);await D.sleep(300);
    while(document.getElementById('t-play').textContent!=='▶')await D.sleep(100);await D.sleep(1500);
    await D.hover('#card-a');await D.sleep(2200);await D.hover('#meta');await D.sleep(1800);`);

  chapter('03 Race Condition');
  await key('Escape');await hold(500);
  await act(`await D.card({eyebrow:'MISSION 03 OF 04 · LIVE ENGINE',title:'Race Condition',
    body:'Two clients share one store. A seeded scheduler interleaves their micro-steps, and every history must be linearizable: some real-time order has to explain every answer.',
    watch:['Client 2 is mid-move: it has removed C from the index and not yet put it back.','Client 1 reads in that gap and sees both chambers free, a state that never existed.','Apply lock: the same interleaving replays, and this time it holds.']});`);
  await act(`await D.click('[data-mission=race]');await D.sleep(1200);await D.hover('.option','torn-move');await D.sleep(700);await D.click('#launch');
    while(!document.getElementById('banner').textContent.includes('TORN READ'))await D.sleep(80);await D.sleep(2600);
    await D.hover('#card-c');await D.sleep(1600);await D.click('#t-back');await D.sleep(1700);await D.click('#t-step');await D.sleep(2200);
    await D.click('#extra button','Apply lock');
    while(!document.getElementById('banner').textContent.includes('LOCK HOLDS'))await D.sleep(80);await D.sleep(2600);await D.hover('#card-a');await D.sleep(1600);`);

  chapter('04 Multiverse');
  await key('Escape');await hold(500);
  await act(`await D.card({eyebrow:'MISSION 04 OF 04 · LIVE ENGINE',title:'Multiverse',
    body:'Hundreds of seeded universes at once. Each orb is one run: coral if the bug surfaced, slate if it held. Its thin ring shows which corners of the state space it reached.',
    watch:['Stale-index breaks in most universes. Grouped by a swarm knob, the failure rate barely moves.','The race breaks in 4 of 1,000. Filter to the failures: needles in a haystack.','Drill into one, read its parameters, and replay it.']});`);
  await act(`await D.click('[data-mission=multiverse]');await D.sleep(1200);await D.click('#mv-launch');
    while(document.getElementById('mv-launch').disabled)await D.sleep(100);await D.sleep(2200);
    await D.choose('#mv-arrange','boundary');await D.sleep(2600);
    const orbs=D.coralOrbs();const [ox,oy]=orbs[Math.floor(orbs.length*.6)];await D.clickAt(ox,oy);await D.sleep(3200);
    await D.click('#mv-detail button','Close');await D.sleep(700);
    await D.click('#mv-race');await D.sleep(500);await D.choose('#mv-count','1000');await D.sleep(300);await D.click('#mv-launch');
    while(document.getElementById('mv-launch').disabled)await D.sleep(100);await D.sleep(2000);
    await D.choose('#mv-arrange','switch');await D.sleep(2000);await D.click('#mv-failing');await D.sleep(2200);
    const orbs2=D.coralOrbs();const [px,py]=orbs2[orbs2.length-1];await D.clickAt(px,py);await D.sleep(3800);
    await D.click('#mv-detail button','Replay in recorder');
    while(!document.getElementById('banner').textContent.includes('TORN READ'))await D.sleep(80);await D.sleep(3200);
    await D.click('#extra button','Back to multiverse');await D.sleep(2600);`);

  chapter('Outro');
  await act(`await D.card({eyebrow:'PULSEFOLD',keep:true,title:'Find it. Shrink it. <span style="white-space:nowrap">Replay it.</span>',
    body:'Seeded simulation with always and sometimes properties, shrinking to minimal replays, and linearizability checking for races, in one dependency-free Rust engine that answers byte for byte the same natively and in the browser.',
    foot:'github.com/copyleftdev/pulsefold',hold:8000});`);
}

const started=Date.now();
try{
  if(SMOKE)await smoke();else await walkthrough();
}catch(e){if(!(e instanceof Stop))throw e}
finally{
  if(enc){enc.stdin.end();await new Promise(r=>enc.on('exit',r))}
  if(encErr)console.error(encErr);
  ws.close();
}
if(logs.length)console.error('page errors:\n'+logs.join('\n'));
if(PLAN){process.stdout.write(JSON.stringify({frames,chapters})+'\n');cleanup();process.exit(0)}
if(WORKER>=0){rmSync(profile,{recursive:true,force:true});cleanup();process.exit(0)}

// --- Chapters: remux the encoded body with chapter metadata ---
const total=Math.round(frames*DT);
const meta=[';FFMETADATA1','title=Pulsefold: flight recorder walkthrough',...chapters.flatMap((c,i)=>['[CHAPTER]','TIMEBASE=1/1000',`START=${c.at}`,`END=${chapters[i+1]?.at??total}`,`title=${c.title}`])].join('\n');
const metaFile=join(OUT,'chapters.txt');writeFileSync(metaFile,meta+'\n');
await new Promise((ok,bad)=>{const p=spawn('ffmpeg',['-y','-loglevel','error','-i',BODY,'-i',metaFile,'-map','0','-map_metadata','1','-map_chapters','1','-c','copy','-movflags','+faststart',FINAL+'.tmp.mp4'],{stdio:'inherit'});
  p.on('exit',c=>c===0?ok():bad(Error('remux failed: '+c)))});
renameSync(FINAL+'.tmp.mp4',FINAL);rmSync(BODY,{force:true});
rmSync(profile,{recursive:true,force:true});
console.log(JSON.stringify({video:FINAL,frames,seconds:+(total/1000).toFixed(2),wallSeconds:Math.round((Date.now()-started)/1000),
  chapters:chapters.map(c=>`${String(Math.floor(c.at/60000)).padStart(2,'0')}:${String(Math.floor(c.at/1000)%60).padStart(2,'0')} ${c.title}`)},null,1));
cleanup();process.exit(0);
