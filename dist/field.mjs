// The multiverse: one orb per seed. Fill says whether the planted bug surfaced; the thin ring
// says which sometimes-properties that universe reached. Arrangement and ignition are motion
// only; the settled field carries the data.
import {ease,prefersReducedMotion} from './fx.mjs';

const C={frost:'#edf5f7',mist:'#adc2ca',cyan:'#8cd8f8',coral:'#ff928a',lift:'#2d4050',slate:'#3b5468',line:'#486174',deep:'#111b23'};
const MONO='ui-monospace,SFMono-Regular,Consolas,monospace';
const rgba=(hex,a)=>{const n=parseInt(hex.slice(1),16);return `rgba(${n>>16},${n>>8&255},${n&255},${a})`};

export class Field{
  constructor(canvas,{onSelect,onHover}){
    this.canvas=canvas;this.ctx=canvas.getContext('2d');this.onSelect=onSelect;this.onHover=onHover;
    this.orbs=[];this.groups=[];this.props=[];this.hover=-1;this.selected=-1;this.failingOnly=false;
    this.reduced=prefersReducedMotion();this.raf=0;this.margins={l:300,r:24,t:84,b:72};
    this.resize();
    new ResizeObserver(()=>{this.resize();this.layout();this.request()}).observe(canvas);
    canvas.addEventListener('pointermove',e=>{const i=this.hit(e.offsetX,e.offsetY);if(i!==this.hover){this.hover=i;this.onHover?.(i>=0?this.orbs[i]:null,e);this.request()}else if(i>=0)this.onHover?.(this.orbs[i],e)});
    canvas.addEventListener('pointerleave',()=>{this.hover=-1;this.onHover?.(null);this.request()});
    canvas.addEventListener('click',e=>{const i=this.hit(e.offsetX,e.offsetY);if(i>=0)this.select(i)});
  }
  setReduced(on){this.reduced=on;this.request()}
  setMargins(m){this.margins=m;this.layout();this.request()}
  resize(){
    const dpr=Math.min(devicePixelRatio||1,2),w=this.canvas.clientWidth,h=this.canvas.clientHeight;
    this.canvas.width=Math.round(w*dpr);this.canvas.height=Math.round(h*dpr);this.ctx.setTransform(dpr,0,0,dpr,0,0);this.w=w;this.h=h;
  }
  /** Empties the field; `expected` is how many universes are coming, so the grid can take its
   *  final shape at once and each universe ignites in its permanent place. */
  clear(expected=0){this.orbs=[];this.groups=[];this.expected=expected;this.hover=this.selected=-1;this.request()}
  /** Adds universes as they come back from the engine; each ignites in turn. */
  add(list,props){
    this.props=props;const now=performance.now();
    for(const [k,u] of list.entries())this.orbs.push({...u,born:now+(this.reduced?0:k*14),x:null,y:null,tx:0,ty:0});
    this.layout();this.request();
  }
  arrange(key,label){this.key=key;this.keyLabel=label;this.layout();this.request()}
  setFailingOnly(on){this.failingOnly=on;this.request()}
  select(i){this.selected=i;this.onSelect?.(i>=0?this.orbs[i]:null,i);this.request()}

  layout(){
    const {l,r,t,b}=this.margins,W=Math.max(120,this.w-l-r),H=Math.max(120,this.h-t-b);
    const key=this.key??'seed';
    const values=key==='seed'?['all']:[...new Set(this.orbs.map(o=>o.group[key]))].sort((a,b)=>(+a-+b)||String(a).localeCompare(b));
    const members=values.map(v=>this.orbs.filter(o=>key==='seed'||o.group[key]===v));
    const gap=key==='seed'?0:18,head=key==='seed'?0:46,bands=values.length;
    const bw=(W-gap*(bands-1))/bands;
    // Largest cell that fits every band.
    const N=Math.max(this.orbs.length,this.expected||0),seedOrder=key==='seed';
    const sizes=members.map(m=>seedOrder?N:m.length);
    let c=Math.min(46,Math.floor(Math.sqrt(W*H/Math.max(1,N))));
    for(;c>4;c--){if(sizes.every(n=>{const cols=Math.max(1,Math.floor(bw/c));return Math.ceil(n/cols)*c+head<=H}))break}
    this.cell=c;
    this.groups=values.map((v,gi)=>{
      const x0=l+gi*(bw+gap),cols=Math.max(1,Math.floor(bw/c)),list=members[gi];
      const used=Math.min(cols,seedOrder?N:list.length)*c,ox=x0+(bw-used)/2;
      list.forEach((o,k)=>{o.tx=ox+(k%cols)*c+c/2;o.ty=t+head+Math.floor(k/cols)*c+c/2;if(o.x===null){o.x=o.tx;o.y=o.ty}});
      const failing=list.filter(o=>o.failed).length;
      return {value:v,x:x0,w:bw,n:list.length,failing};
    });
    if(this.reduced)for(const o of this.orbs){o.x=o.tx;o.y=o.ty}
  }

  hit(x,y){
    const r=this.cell*.5;let best=-1,bd=r*r;
    this.orbs.forEach((o,i)=>{const d=(o.x-x)**2+(o.y-y)**2;if(d<bd){bd=d;best=i}});
    return best;
  }

  request(){if(!this.raf)this.raf=requestAnimationFrame(t=>{this.raf=0;this.draw(t)})}

  draw(now){
    const ctx=this.ctx,c=this.cell??20;let busy=false;
    ctx.clearRect(0,0,this.w,this.h);
    // Group headers: knob value, failure count and a rate bar.
    if(this.key&&this.key!=='seed'){
      for(const g of this.groups){
        const y=this.margins.t+4;
        ctx.font=`700 12px ${MONO}`;ctx.fillStyle=C.frost;ctx.textAlign='left';ctx.textBaseline='top';
        ctx.fillText(`${this.keyLabel} ${g.value}`,g.x,y);
        ctx.font=`11px ${MONO}`;ctx.fillStyle=C.mist;ctx.fillText(`${g.failing}/${g.n} failing`,g.x,y+16);
        ctx.fillStyle=C.lift;ctx.fillRect(g.x,y+32,g.w,3);
        ctx.fillStyle=C.coral;ctx.fillRect(g.x,y+32,g.w*(g.n?g.failing/g.n:0),3);
      }
    }
    const k=this.reduced?1:.16,r=Math.max(1.5,c*.27),ring=c>=12;
    for(const [i,o] of this.orbs.entries()){
      const dx=o.tx-o.x,dy=o.ty-o.y;if(Math.abs(dx)+Math.abs(dy)>.3){o.x+=dx*k;o.y+=dy*k;busy=true}else{o.x=o.tx;o.y=o.ty}
      const age=(now-o.born)/420;if(age<1&&!this.reduced)busy=true;
      const grow=this.reduced?1:Math.max(0,Math.min(1,ease.outBack(Math.max(0,age))));
      if(grow<=0){ctx.fillStyle=rgba(C.line,.35);ctx.fillRect(o.tx-1,o.ty-1,2,2);continue}
      const dim=this.failingOnly&&!o.failed,focus=i===this.selected||i===this.hover;
      ctx.globalAlpha=dim?.16:1;
      const rr=r*grow*(focus?1.35:1);
      if(o.failed){
        ctx.fillStyle=C.coral;ctx.beginPath();ctx.arc(o.x,o.y,rr*.62,0,Math.PI*2);ctx.fill();
        ctx.strokeStyle=rgba(C.coral,.75);ctx.lineWidth=1.2;ctx.beginPath();ctx.arc(o.x,o.y,rr,0,Math.PI*2);ctx.stroke();
        if(age<1.6&&!this.reduced){const p=Math.min(1,age/1.6);ctx.strokeStyle=rgba(C.coral,.9*(1-p));ctx.lineWidth=1.5;ctx.beginPath();ctx.arc(o.x,o.y,rr+p*c*.8,0,Math.PI*2);ctx.stroke();busy=true}
      }else{
        ctx.fillStyle='#2b3f50';ctx.beginPath();ctx.arc(o.x,o.y,rr*.82,0,Math.PI*2);ctx.fill();
        ctx.strokeStyle=rgba(C.frost,.28);ctx.lineWidth=1;ctx.stroke();
      }
      if(ring&&grow>=1){
        const n=this.props.length,rr2=rr+c*.11;
        if(focus){
          // The full picture on focus: one segment per property.
          const seg=Math.PI*2/n;ctx.lineWidth=Math.max(1.6,c*.08);
          this.props.forEach((p,j)=>{ctx.strokeStyle=o.props.has(p)?C.frost:rgba(C.line,.6);
            ctx.beginPath();ctx.arc(o.x,o.y,rr2,-Math.PI/2+j*seg+.1,-Math.PI/2+(j+1)*seg-.1);ctx.stroke()});
        }else{
          // At rest: one quiet arc, as long as the share of properties reached.
          ctx.lineWidth=Math.max(1,c*.045);ctx.strokeStyle=rgba(C.frost,.55);
          ctx.beginPath();ctx.arc(o.x,o.y,rr2,-Math.PI/2,-Math.PI/2+Math.PI*2*o.props.size/n);ctx.stroke();
        }
      }
      if(focus){ctx.strokeStyle=C.cyan;ctx.lineWidth=2;ctx.beginPath();ctx.arc(o.x,o.y,rr+c*.24,0,Math.PI*2);ctx.stroke()}
      ctx.globalAlpha=1;
    }
    if(busy)this.request();
  }
}
