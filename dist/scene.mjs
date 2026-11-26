// The flight recorder's world: an orbital dial with two chamber rings. Motion is decoration;
// the settled frame alone carries the data (visual direction, rule 04).
import {ease,prefersReducedMotion} from './fx.mjs';
import {clock,opWindow} from './frames.mjs';

const C={basalt:'#1a2631',deep:'#0f1820',slate:'#233341',lift:'#2d4050',line:'#486174',frost:'#edf5f7',mist:'#adc2ca',
  cyan:'#8cd8f8',mint:'#8de7b5',coral:'#ff928a',amber:'#ffd18b',lilac:'#b7c1d7'};
const GAP=Math.PI/180*30, START=-Math.PI/2+GAP/2, SWEEP=Math.PI*2-GAP;
const MONO='ui-monospace,SFMono-Regular,Consolas,monospace';
const rgba=(hex,a)=>{const n=parseInt(hex.slice(1),16);return `rgba(${n>>16},${n>>8&255},${n&255},${a})`};

function seeded(seed){let s=seed>>>0;return()=>{s=(s+0x6D2B79F5)>>>0;let t=s;t=Math.imul(t^t>>>15,t|1);t^=t+Math.imul(t^t>>>7,t|61);return((t^t>>>14)>>>0)/4294967296}}

export class Scene{
  constructor(canvas){
    this.canvas=canvas;this.ctx=canvas.getContext('2d');
    this.reduced=prefersReducedMotion();
    this.domain=[600,840];this.frame=null;this.mode='title';
    this.items=new Map();this.fx=[];this.particles=[];this.shakeUntil=0;this.shakeAmp=0;
    this.spin=0;this.last=0;this.raf=0;this.wake=()=>this.request();
    const r=seeded(7);this.stars=Array.from({length:180},()=>({x:r(),y:r(),z:.25+r()*.75,tw:r()*6.28}));
    this.hatch=this.makeHatch();
    this.resize();
    new ResizeObserver(()=>{this.resize();this.request()}).observe(canvas);
    document.addEventListener('visibilitychange',()=>{if(!document.hidden)this.request()});
  }
  setReduced(on){this.reduced=on;if(on){this.fx=[];this.particles=[]}this.request()}
  makeHatch(){
    const p=document.createElement('canvas');p.width=p.height=10;const g=p.getContext('2d');
    g.fillStyle=rgba(C.coral,.85);g.fillRect(0,0,10,10);g.strokeStyle=C.deep;g.lineWidth=2.4;
    g.beginPath();for(const o of [-10,0,10]){g.moveTo(o,10);g.lineTo(o+10,0)}g.stroke();
    return this.ctx.createPattern(p,'repeat');
  }
  resize(){
    const dpr=Math.min(devicePixelRatio||1,2),w=this.canvas.clientWidth,h=this.canvas.clientHeight;
    this.canvas.width=Math.round(w*dpr);this.canvas.height=Math.round(h*dpr);
    this.ctx.setTransform(dpr,0,0,dpr,0,0);this.w=w;this.h=h;
    const narrow=w<760,top=narrow?96:76,bottom=narrow?232:186;
    const avail=Math.max(160,h-top-bottom);
    this.cx=w/2;this.cy=top+avail/2;
    this.R=Math.max(70,Math.min(narrow?w*.38:w*.25,avail*.4));
  }
  geometry(){const R=this.R;return {r1:R*.64,r2:R*.88,bw:R*.13,hub:R*.42,orbit:R*1.13}}
  angle(t){const [lo,hi]=this.domain;return START+(t-lo)/(hi-lo)*SWEEP}
  radius(room){const g=this.geometry();return room===1?g.r1:g.r2}

  setMode(mode){this.mode=mode;if(mode==='title'){this.frame=null;this.items.clear()}this.request()}
  setDomain(d){this.domain=d;this.request()}

  /** Shows a frame: arcs glide to their new places; `cue` adds decoration for this step. */
  show(frame,{instant=false,cue=true}={}){
    const prev=this.frame;this.frame=frame;
    const want=new Map();
    const add=(layer,list)=>{const seen={};for(const s of list){const n=seen[s.id]=(seen[s.id]??0)+1;want.set(`${layer}|${s.id}|${n}`,{layer,...s})}};
    add('ledger',frame.ledger);add('index',frame.index);
    for(const [k,it] of this.items)if(!want.has(k)){it.target.alpha=0;it.dying=true}
    for(const [k,s] of want){
      let it=this.items.get(k);
      if(!it){it={cur:{lo:s.lo,hi:s.hi,room:s.room,alpha:0},target:{},born:performance.now()};this.items.set(k,it)}
      it.dying=false;it.layer=s.layer;it.id=s.id;
      Object.assign(it.target,{lo:s.lo,hi:s.hi,room:s.room,alpha:1});
      if(instant||this.reduced)Object.assign(it.cur,it.target);
    }
    if(cue&&!instant&&!this.reduced)this.cue(prev,frame);
    this.request();
  }

  cue(prev,f){
    const now=performance.now();
    if(f.kind==='op'){
      const bad=f.probes.filter(p=>!p.agree);
      this.fx.push({type:'sweep',t0:now,dur:700,color:f.verdict==='FAIL'?C.coral:C.cyan,pings:bad.map(p=>(p.lo+p.hi)/2)});
      if(f.result==='CONFLICT'){const a=f.probes.find(p=>p.name==='attempted');this.fx.push({type:'ripple',t0:now,dur:650,lo:a.lo,hi:a.hi,color:C.amber})}
      if(f.verdict==='FAIL')this.failure(now,bad.map(p=>(p.lo+p.hi)/2));
      else this.fx.push({type:'hubpulse',t0:now,dur:500,color:C.mint});
    }else{
      if(f.micro==='read'||f.micro==='check'){const w=opWindow(f.op);this.fx.push({type:'beam',t0:now,dur:520,client:f.client,lo:w.lo,hi:w.hi,color:f.micro==='read'?C.cyan:C.mist})}
      if(['write-ledger','index','reindex','unindex'].includes(f.micro)){const w=opWindow(f.op);this.fx.push({type:'beam',t0:now,dur:420,client:f.client,lo:w.lo,hi:w.hi,color:C.frost,solid:true})}
      if(f.micro==='acquire'||f.micro==='release')this.fx.push({type:'hubpulse',t0:now,dur:420,color:f.micro==='acquire'?C.cyan:C.mist});
      if(f.micro==='read'&&f.ghosts.length+f.missing.length>0)this.failure(now,[]);
    }
  }
  failure(now,angles){
    this.fx.push({type:'shock',t0:now,dur:900,color:C.coral});
    this.shakeUntil=now+180;this.shakeAmp=2;
    const {hub}=this.geometry();
    for(let i=0;i<14;i++){const a=Math.random()*Math.PI*2,v=60+Math.random()*120;
      this.particles.push({x:this.cx+Math.cos(a)*hub*.6,y:this.cy+Math.sin(a)*hub*.6,vx:Math.cos(a)*v,vy:Math.sin(a)*v,life:1,color:C.coral})}
    for(const t of angles){const a=this.angle(t),g=this.geometry();
      for(let i=0;i<6;i++){const s=Math.random()*Math.PI*2,v=30+Math.random()*60;
        this.particles.push({x:this.cx+Math.cos(a)*(g.r1+g.r2)/2,y:this.cy+Math.sin(a)*(g.r1+g.r2)/2,vx:Math.cos(s)*v,vy:Math.sin(s)*v,life:.8,color:C.coral})}}
  }
  wave(color){if(!this.reduced)this.fx.push({type:'wave',t0:performance.now(),dur:1100,color});this.request()}
  scan(on){this.scanning=on;this.request()}

  request(){if(!this.raf)this.raf=requestAnimationFrame(t=>{this.raf=0;this.draw(t)})}

  draw(now){
    const dt=Math.min(.05,(now-(this.last||now))/1000);this.last=now;
    const ctx=this.ctx,{w,h}=this;
    let busy=false;
    // Settle tweens.
    const k=1-Math.exp(-dt*11);
    for(const [key,it] of this.items){
      for(const f of ['lo','hi','room','alpha']){const d=it.target[f]-it.cur[f];if(Math.abs(d)>.002){it.cur[f]+=d*k;busy=true}else it.cur[f]=it.target[f]}
      if(it.dying&&it.cur.alpha<=.01)this.items.delete(key);
    }
    const ambient=!this.reduced&&!document.hidden;
    if(ambient)this.spin+=dt*(this.scanning?2.6:.035);
    ctx.save();
    // Shake.
    if(now<this.shakeUntil&&!this.reduced){ctx.translate((Math.random()-.5)*this.shakeAmp*2,(Math.random()-.5)*this.shakeAmp*2);busy=true}
    this.background(ctx,now,ambient);
    this.dial(ctx);
    if(this.frame){this.probes(ctx);this.arcs(ctx);if(this.frame.kind==='micro')this.drones(ctx,now)}
    this.hub(ctx,now);
    // Labels after the hub and every band, so nothing covers them.
    if(this.frame)for(const draw of this.pending??[])draw();
    busy=this.effects(ctx,now,dt)||busy;
    if(this.scanning)this.scanBeam(ctx,.28);
    else if(this.mode==='title'&&!this.reduced)this.scanBeam(ctx,.07);
    ctx.restore();
    if(busy||ambient||this.scanning)this.request();
  }

  background(ctx,now,ambient){
    const {w,h}=this;
    const g=ctx.createRadialGradient(this.cx,this.cy,this.R*.2,this.cx,this.cy,Math.max(w,h)*.75);
    g.addColorStop(0,'#1e2d3a');g.addColorStop(.55,C.basalt);g.addColorStop(1,'#111b23');
    ctx.fillStyle=g;ctx.fillRect(-4,-4,w+8,h+8);
    for(const s of this.stars){
      const x=((s.x*w+(ambient?now*.004*s.z:0))%w+w)%w,y=s.y*h;
      const a=.18+.22*s.z*(ambient?(.75+.25*Math.sin(now*.0012+s.tw)):1);
      ctx.fillStyle=rgba(C.mist,a);ctx.fillRect(x,y,s.z*1.4,s.z*1.4);
    }
  }

  dial(ctx){
    const {cx,cy,R}=this,g=this.geometry(),[lo,hi]=this.domain;
    // Outer bezel with slow-turning micro-ticks.
    ctx.lineWidth=1;ctx.strokeStyle=rgba(C.line,.55);
    ctx.beginPath();ctx.arc(cx,cy,R*1.02,0,Math.PI*2);ctx.stroke();
    ctx.strokeStyle=rgba(C.line,.35);
    for(let i=0;i<120;i++){const a=i/120*Math.PI*2+this.spin*.2,r0=R*1.25,r1=R*(i%10?1.27:1.3);
      ctx.beginPath();ctx.moveTo(cx+Math.cos(a)*r0,cy+Math.sin(a)*r0);ctx.lineTo(cx+Math.cos(a)*r1,cy+Math.sin(a)*r1);ctx.stroke()}
    // Chamber tracks.
    for(const [room,r] of [[1,g.r1],[2,g.r2]]){
      ctx.lineWidth=g.bw;ctx.strokeStyle=rgba('#15212a',.9);ctx.lineCap='butt';
      ctx.beginPath();ctx.arc(cx,cy,r,START,START+SWEEP);ctx.stroke();
      ctx.lineWidth=1;ctx.strokeStyle=rgba(C.line,.6);
      for(const e of [-1,1]){ctx.beginPath();ctx.arc(cx,cy,r+e*g.bw/2,START,START+SWEEP);ctx.stroke()}
      if(this.mode==='title')continue;
      const a=START-.05;
      ctx.fillStyle=C.mist;ctx.font=`600 ${Math.max(10,R*.055)}px ${MONO}`;ctx.textAlign='right';ctx.textBaseline='middle';
      ctx.save();ctx.translate(cx+Math.cos(a)*r,cy+Math.sin(a)*r);ctx.fillText('C'+room,-2,0);ctx.restore();
    }
    // Time ticks every 15 ticks, labels on the hour.
    if(this.mode==='title')return;
    const step=hi-lo>480?30:15;
    for(let t=lo;t<=hi;t+=step){
      const a=this.angle(t),hour=t%60===0,r0=g.r2+g.bw/2+3,r1=r0+(hour?10:5);
      ctx.strokeStyle=rgba(C.mist,hour?.7:.35);ctx.lineWidth=1;
      ctx.beginPath();ctx.moveTo(cx+Math.cos(a)*r0,cy+Math.sin(a)*r0);ctx.lineTo(cx+Math.cos(a)*r1,cy+Math.sin(a)*r1);ctx.stroke();
      if(hour){ctx.fillStyle=C.mist;ctx.font=`${Math.max(10,R*.05)}px ${MONO}`;ctx.textAlign='center';ctx.textBaseline='middle';
        const rl=r1+R*.07;ctx.fillText(clock(t),cx+Math.cos(a)*rl,cy+Math.sin(a)*rl)}
    }
  }

  wedge(ctx,lo,hi,r0,r1){
    const a0=this.angle(lo),a1=this.angle(hi);
    ctx.beginPath();ctx.arc(this.cx,this.cy,r1,a0,a1);ctx.arc(this.cx,this.cy,r0,a1,a0,true);ctx.closePath();
  }

  probes(ctx){
    const f=this.frame;if(!f.probes?.length)return;
    const g=this.geometry(),r0=g.r1-g.bw/2-6,r1=g.r2+g.bw/2+6;
    for(const p of f.probes){
      const bad=!p.agree,rejected=p.name==='attempted'&&f.result==='CONFLICT';
      this.wedge(ctx,p.lo,p.hi,r0,r1);
      ctx.fillStyle=bad?rgba(C.coral,.13):rgba(C.frost,.045);ctx.fill();
      ctx.lineWidth=bad?1.6:1;ctx.setLineDash(bad?[]:[3,4]);
      const tone=bad?C.coral:rejected?C.amber:C.mist;
      ctx.strokeStyle=bad?C.coral:rgba(tone,.75);ctx.stroke();ctx.setLineDash([]);
      // Tag outside the dial: which window, whether it agrees, and both answers.
      // Race frames park a drone at the window's middle, so their tag sits past the window's end.
      const a=f.kind==='micro'?Math.min(START+SWEEP,this.angle(p.hi)+.16):this.angle((p.lo+p.hi)/2),rt=this.R*1.25;
      const fs=Math.max(10,this.R*.046),room=v=>v==='none'?'none':v.replace(',','+');
      const head=`${p.name.toUpperCase()} ${bad?'≠':'='}`;
      const body=p.name==='read'?`saw ${room(p.impl)} · ledger ${room(p.ref)}`:`ref ${room(p.ref)} · index ${room(p.impl)}`;
      ctx.font=`${fs*.92}px ${MONO}`;const tw=ctx.measureText(body).width;
      // Anchor by side, then keep the whole tag on screen.
      const side=Math.cos(a)>.25?1:Math.cos(a)<-.25?-1:0;
      let x=this.cx+Math.cos(a)*rt;const y=this.cy+Math.sin(a)*rt;
      let left=side===1?x:side===-1?x-tw:x-tw/2;left=Math.max(8,Math.min(this.w-8-tw,left));x=left;
      ctx.textAlign='left';ctx.textBaseline='middle';
      ctx.font=`700 ${fs}px ${MONO}`;ctx.fillStyle=tone;ctx.fillText(head,side===-1?x+tw-ctx.measureText(head).width:side===0?x+(tw-ctx.measureText(head).width)/2:x,y-fs*.7);
      ctx.font=`${fs*.92}px ${MONO}`;ctx.fillStyle=bad?C.frost:C.mist;ctx.fillText(body,x,y+fs*.7);
    }
  }

  band(ctx,it,width,style){
    const g=this.geometry(),r=g.r1+(g.r2-g.r1)*(it.cur.room-1);
    const a0=this.angle(it.cur.lo),a1=this.angle(it.cur.hi);
    ctx.lineWidth=width;ctx.strokeStyle=style;ctx.lineCap='butt';
    ctx.beginPath();ctx.arc(this.cx,this.cy,r,a0,Math.max(a0+.002,a1));ctx.stroke();
    return {r,a0,a1};
  }

  label(ctx,text,a,r,color){
    ctx.font=`600 ${Math.max(10,this.R*.052)}px ${MONO}`;ctx.fillStyle=color;ctx.textAlign='center';ctx.textBaseline='middle';
    ctx.fillText(text,this.cx+Math.cos(a)*r,this.cy+Math.sin(a)*r);
  }

  /** A small status tag on a dark pill, so it stays legible over rings and wedges. */
  tag(ctx,text,a,r,color){
    const fs=Math.max(9,this.R*.042);ctx.font=`700 ${fs}px ${MONO}`;
    const x=this.cx+Math.cos(a)*r,y=this.cy+Math.sin(a)*r,w=ctx.measureText(text).width+fs;
    ctx.fillStyle=rgba('#111b23',.85);ctx.beginPath();
    if(ctx.roundRect)ctx.roundRect(x-w/2,y-fs*.85,w,fs*1.7,fs*.4);else ctx.rect(x-w/2,y-fs*.85,w,fs*1.7);
    ctx.fill();ctx.strokeStyle=rgba(color,.6);ctx.lineWidth=1;ctx.stroke();
    ctx.fillStyle=color;ctx.textAlign='center';ctx.textBaseline='middle';ctx.fillText(text,x,y+.5);
  }

  arcs(ctx){
    const f=this.frame,g=this.geometry();
    const key=s=>`${s.id}|${s.room}|${s.lo}|${s.hi}`;
    const ghost=new Set(f.ghosts.map(key)),missing=new Set(f.missing.map(key));
    const tkey=it=>`${it.id}|${it.target.room}|${it.target.lo}|${it.target.hi}`;
    const labels=this.pending=[];
    // Bookings.
    for(const it of this.items.values()){
      if(it.layer!=='ledger')continue;
      ctx.globalAlpha=it.cur.alpha;
      const lost=missing.has(tkey(it));
      const {r,a0,a1}=this.band(ctx,it,g.bw-4,lost?rgba('#3b5468',.55):'#3b5468');
      ctx.lineWidth=1.3;ctx.strokeStyle=lost?C.mist:rgba(C.frost,.5);ctx.setLineDash(lost?[5,4]:[]);
      for(const e of [-1,1]){ctx.beginPath();ctx.arc(this.cx,this.cy,r+e*(g.bw/2-2),a0,a1);ctx.stroke()}
      for(const a of [a0,a1]){ctx.beginPath();ctx.moveTo(this.cx+Math.cos(a)*(r-g.bw/2+2),this.cy+Math.sin(a)*(r-g.bw/2+2));ctx.lineTo(this.cx+Math.cos(a)*(r+g.bw/2-2),this.cy+Math.sin(a)*(r+g.bw/2-2));ctx.stroke()}
      ctx.setLineDash([]);
      labels.push(()=>this.label(ctx,it.id,(a0+a1)/2,r,C.frost));
      if(lost)labels.push(()=>this.tag(ctx,'UNINDEXED',(a0+a1)/2,r-g.bw*1.02,C.mist));
    }
    // Index entries: a bright trace on the booking it describes, or a hatched ghost.
    for(const it of this.items.values()){
      if(it.layer!=='index')continue;
      ctx.globalAlpha=it.cur.alpha;
      if(ghost.has(tkey(it))){
        const {r,a0,a1}=this.band(ctx,it,g.bw*.5,this.hatch);
        ctx.shadowColor=C.coral;ctx.shadowBlur=this.reduced?0:12;
        this.band(ctx,it,2,C.coral);ctx.shadowBlur=0;
        labels.push(()=>this.tag(ctx,`GHOST ${it.id}`,(a0+a1)/2,r-g.bw*1.02,C.coral));
      }else{
        ctx.shadowColor=C.frost;ctx.shadowBlur=this.reduced?0:6;
        this.band(ctx,it,2.2,rgba(C.frost,.9));ctx.shadowBlur=0;
      }
    }
    ctx.globalAlpha=1;

  }

  hub(ctx,now){
    const {cx,cy}=this,g=this.geometry(),f=this.frame;
    const grad=ctx.createRadialGradient(cx,cy,0,cx,cy,g.hub);
    grad.addColorStop(0,'#26384a');grad.addColorStop(1,'#192632');
    ctx.fillStyle=grad;ctx.beginPath();ctx.arc(cx,cy,g.hub,0,Math.PI*2);ctx.fill();
    ctx.lineWidth=1;ctx.strokeStyle=rgba(C.line,.8);ctx.stroke();
    // Rotating inner reticle.
    ctx.save();ctx.translate(cx,cy);ctx.rotate(this.spin);ctx.strokeStyle=rgba(C.cyan,.22);ctx.setLineDash([2,7]);
    ctx.beginPath();ctx.arc(0,0,g.hub*.86,0,Math.PI*2);ctx.stroke();ctx.restore();ctx.setLineDash([]);
    ctx.textAlign='center';ctx.textBaseline='middle';
    const big=Math.max(18,g.hub*.42),small=Math.max(9,g.hub*.12);
    if(!f)return;
    if(f.kind==='op'){
      const bad=f.verdict==='FAIL';
      ctx.fillStyle=bad?C.coral:C.mint;ctx.font=`700 ${big}px ${MONO}`;ctx.fillText(bad?'×':'✓',cx,cy-small*.6);
      ctx.fillStyle=C.mist;ctx.font=`${small}px ${MONO}`;ctx.fillText(`TURN ${f.turn} · STEP ${f.step}`,cx,cy+big*.62);
    }else{
      const held=f.lock;
      ctx.strokeStyle=held?C.cyan:C.mist;ctx.lineWidth=2;
      const s=big*.42;ctx.beginPath();ctx.arc(cx,cy-s*.9,s*.55,Math.PI,0);ctx.stroke();
      ctx.fillStyle=held?rgba(C.cyan,.25):'transparent';ctx.fillRect(cx-s*.8,cy-s*.9,s*1.6,s*1.15);ctx.strokeRect(cx-s*.8,cy-s*.9,s*1.6,s*1.15);
      ctx.fillStyle=held?C.cyan:C.mist;ctx.font=`600 ${small}px ${MONO}`;
      ctx.fillText(held?(held==='setup'?'LOCK · SETUP':`LOCK · CLIENT ${held}`):'LOCK · FREE',cx,cy+s*.9);
      ctx.fillStyle=C.mist;ctx.fillText(`t = ${f.time}`,cx,cy+s*.9+small*1.5);
    }
  }

  dronePos(client){
    const f=this.frame,g=this.geometry(),op=f.active[client]??f.last?.[client]??null;
    const n=+client||0,base=op?(()=>{const w=opWindow(op);return this.angle((w.lo+w.hi)/2)})():START+SWEEP*(n/4);
    const r=g.orbit+(n%2)*this.R*.09;
    return {x:this.cx+Math.cos(base)*r,y:this.cy+Math.sin(base)*r,a:base,op};
  }
  drones(ctx,now){
    const f=this.frame;
    const clients=[...new Set(Object.keys(f.active).concat(f.client))].filter(c=>c!=='setup').sort();
    for(const c of clients){
      const p=this.dronePos(c),acting=f.client===c,s=Math.max(6,this.R*.035);
      if(acting&&!this.reduced){ctx.fillStyle=rgba(C.cyan,.12+.08*Math.sin(now*.01));ctx.beginPath();ctx.arc(p.x,p.y,s*2.6,0,Math.PI*2);ctx.fill()}
      ctx.save();ctx.translate(p.x,p.y);ctx.rotate(Math.PI/4);
      ctx.fillStyle=acting?C.cyan:C.lift;ctx.strokeStyle=acting?C.frost:C.mist;ctx.lineWidth=1.2;
      ctx.fillRect(-s,-s,s*2,s*2);ctx.strokeRect(-s,-s,s*2,s*2);ctx.restore();
      ctx.font=`700 ${s*1.2}px ${MONO}`;ctx.textAlign='center';ctx.textBaseline='middle';
      ctx.fillStyle=acting?C.basalt:C.frost;ctx.fillText(c,p.x,p.y+.5);
      if(this.w<760)continue; // The diamond carries the client number; there is no room for labels.
      ctx.fillStyle=acting?C.cyan:C.mist;ctx.font=`600 ${Math.max(9,this.R*.045)}px ${MONO}`;
      const ca=Math.cos(p.a);ctx.textAlign=ca>.3?'left':ca<-.3?'right':'center';
      const lx=p.x+ca*s*2.6,ly=p.y+Math.sin(p.a)*s*2.6+(Math.abs(ca)<=.3?Math.sign(Math.sin(p.a))*s:0);ctx.fillText(`CLIENT ${c}`,lx,ly);
      if(p.op&&acting&&f.micro!=='read'){ctx.fillStyle=C.mist;ctx.font=`${Math.max(9,this.R*.04)}px ${MONO}`;ctx.fillText(f.micro.toUpperCase(),lx,ly+Math.max(11,this.R*.055))}
    }
  }

  effects(ctx,now,dt){
    let busy=false;const g=this.geometry();
    this.fx=this.fx.filter(e=>{
      const p=(now-e.t0)/e.dur;if(p>=1)return false;busy=true;
      if(e.type==='sweep'){
        const [lo,hi]=this.domain,t=lo+(hi-lo)*ease.inOut(p),a=this.angle(t);
        const grad=ctx.createConicGradient?ctx.createConicGradient(a-.5,this.cx,this.cy):null;
        if(grad){grad.addColorStop(0,'transparent');grad.addColorStop(.0795,rgba(e.color,.22));grad.addColorStop(.08,'transparent');
          ctx.fillStyle=grad;ctx.beginPath();ctx.arc(this.cx,this.cy,g.r2+g.bw,0,Math.PI*2);ctx.arc(this.cx,this.cy,g.hub,0,Math.PI*2,true);ctx.fill()}
        ctx.strokeStyle=rgba(e.color,.8);ctx.lineWidth=1.5;ctx.beginPath();
        ctx.moveTo(this.cx+Math.cos(a)*g.hub,this.cy+Math.sin(a)*g.hub);ctx.lineTo(this.cx+Math.cos(a)*(g.r2+g.bw),this.cy+Math.sin(a)*(g.r2+g.bw));ctx.stroke();
        for(const ping of e.pings){if(ping<=t){const q=Math.min(1,(t-ping)/((hi-lo)*.25)),pa=this.angle(ping),pr=(g.r1+g.r2)/2;
          ctx.strokeStyle=rgba(C.coral,1-q);ctx.lineWidth=2;ctx.beginPath();ctx.arc(this.cx+Math.cos(pa)*pr,this.cy+Math.sin(pa)*pr,6+q*this.R*.18,0,Math.PI*2);ctx.stroke()}}
      }else if(e.type==='ripple'){
        const r0=g.r1-g.bw/2-6,r1=g.r2+g.bw/2+6+ease.outExpo(p)*this.R*.12;
        this.wedge(ctx,e.lo,e.hi,r0,r1);ctx.strokeStyle=rgba(e.color,(1-p)*.9);ctx.lineWidth=2;ctx.stroke();
      }else if(e.type==='shock'){
        ctx.strokeStyle=rgba(e.color,(1-p)*.8);ctx.lineWidth=2+3*(1-p);
        ctx.beginPath();ctx.arc(this.cx,this.cy,g.hub+ease.outExpo(p)*this.R*1.1,0,Math.PI*2);ctx.stroke();
      }else if(e.type==='hubpulse'){
        ctx.strokeStyle=rgba(e.color,(1-p)*.7);ctx.lineWidth=2;ctx.beginPath();ctx.arc(this.cx,this.cy,g.hub*(1+.18*ease.outExpo(p)),0,Math.PI*2);ctx.stroke();
      }else if(e.type==='wave'){
        const a=START+SWEEP*ease.inOut(p);
        for(const r of [g.r1,g.r2]){ctx.strokeStyle=rgba(e.color,.55*(1-p*.6));ctx.lineWidth=g.bw*.35;ctx.beginPath();ctx.arc(this.cx,this.cy,r,START,a);ctx.stroke()}
      }else if(e.type==='beam'){
        const d=this.dronePos(e.client),a0=this.angle(e.lo),a1=this.angle(e.hi),am=(a0+a1)/2,tr=g.r2+g.bw/2;
        ctx.strokeStyle=rgba(e.color,(1-p)*.9);ctx.lineWidth=1.5;ctx.setLineDash(e.solid?[]:[4,4]);
        ctx.beginPath();ctx.moveTo(d.x,d.y);ctx.lineTo(this.cx+Math.cos(am)*tr,this.cy+Math.sin(am)*tr);ctx.stroke();ctx.setLineDash([]);
        this.wedge(ctx,e.lo,e.hi,g.r1-g.bw/2,tr);ctx.fillStyle=rgba(e.color,.1*(1-p));ctx.fill();
      }
      return true;
    });
    this.particles=this.particles.filter(q=>{
      q.life-=dt*1.4;if(q.life<=0)return false;busy=true;
      q.x+=q.vx*dt;q.y+=q.vy*dt;q.vx*=.96;q.vy*=.96;
      ctx.fillStyle=rgba(q.color,q.life);ctx.fillRect(q.x-1.5,q.y-1.5,3,3);return true;
    });
    return busy;
  }

  scanBeam(ctx,alpha){
    const g=this.geometry(),a=this.spin*(this.scanning?2.4:6);
    const grad=ctx.createConicGradient?ctx.createConicGradient(a-.9,this.cx,this.cy):null;
    if(grad){grad.addColorStop(0,'transparent');grad.addColorStop(.14,rgba(C.cyan,alpha));grad.addColorStop(.1405,'transparent');
      ctx.fillStyle=grad;ctx.beginPath();ctx.arc(this.cx,this.cy,g.r2+g.bw,0,Math.PI*2);ctx.arc(this.cx,this.cy,g.hub,0,Math.PI*2,true);ctx.fill()}
  }
}
