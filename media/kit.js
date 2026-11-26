// Injected into the page by director.mjs: an on-screen cursor, section cards and a few
// helpers. Media only; nothing here ships in dist/.
(()=>{
  if(window.__dir)return;
  const css=`
  #dir-cursor{position:fixed;left:0;top:0;width:30px;height:30px;z-index:100000;pointer-events:none;will-change:transform;filter:drop-shadow(0 3px 6px rgba(0,0,0,.55))}
  #dir-ripple{position:fixed;z-index:99999;pointer-events:none;width:44px;height:44px;margin:-22px 0 0 -22px;border-radius:50%;border:2px solid #8cd8f8;opacity:0}
  #dir-ripple.go{animation:dirr .55s cubic-bezier(.16,1,.3,1)}
  @keyframes dirr{0%{opacity:.95;transform:scale(.35)}100%{opacity:0;transform:scale(1.5)}}
  #dir-card{position:fixed;inset:0;z-index:99998;display:grid;place-items:center;pointer-events:none;opacity:0;transition:opacity .7s cubic-bezier(.16,1,.3,1);
    background:radial-gradient(ellipse at center,#1b2834,#0d151c 75%)}
  #dir-card.on{opacity:1}
  #dir-card .in{max-width:1080px;padding:0 64px;transform:translateY(14px);transition:transform .9s cubic-bezier(.16,1,.3,1)}
  #dir-card.on .in{transform:none}
  #dir-card .eb{font:600 20px/1.4 ui-monospace,SFMono-Regular,Consolas,monospace;letter-spacing:.2em;color:#8cd8f8;margin:0 0 18px}
  #dir-card h1{font:800 88px/1 system-ui,-apple-system,sans-serif;letter-spacing:-.045em;color:#edf5f7;margin:0 0 26px}
  #dir-card h1 span{color:#8cd8f8}
  #dir-card p.body{font:400 30px/1.45 system-ui,-apple-system,sans-serif;color:#cfdde3;margin:0 0 30px;max-width:30em}
  #dir-card .wk{font:600 17px ui-monospace,SFMono-Regular,Consolas,monospace;letter-spacing:.18em;color:#adc2ca;margin:0 0 12px}
  #dir-card ol{margin:0;padding:0;list-style:none;counter-reset:w}
  #dir-card li{font:400 25px/1.45 system-ui,-apple-system,sans-serif;color:#edf5f7;margin:0 0 12px;padding-left:52px;position:relative;opacity:0;transform:translateX(-10px);transition:opacity .6s,transform .6s cubic-bezier(.16,1,.3,1)}
  #dir-card.on li{opacity:1;transform:none}
  #dir-card li::before{counter-increment:w;content:counter(w,decimal-leading-zero);position:absolute;left:0;top:3px;font:700 18px ui-monospace,SFMono-Regular,Consolas,monospace;color:#8cd8f8}
  #dir-card .foot{margin-top:34px;font:600 22px ui-monospace,SFMono-Regular,Consolas,monospace;color:#8de7b5;letter-spacing:.04em}
  #dir-card .rule{height:3px;width:0;background:#8cd8f8;border-radius:2px;margin:0 0 30px;transition:width 1.1s cubic-bezier(.16,1,.3,1)}
  #dir-card.on .rule{width:120px}`;
  const st=document.createElement('style');st.textContent=css;document.head.append(st);
  const cur=document.createElement('div');cur.id='dir-cursor';
  cur.innerHTML='<svg width="30" height="30" viewBox="0 0 30 30"><path d="M4 2 L4 24 L10 18.5 L14.2 27.5 L18.2 25.6 L14 17 L22 17 Z" fill="#edf5f7" stroke="#111b23" stroke-width="1.6" stroke-linejoin="round"/></svg>';
  const rip=document.createElement('div');rip.id='dir-ripple';
  const card=document.createElement('div');card.id='dir-card';
  document.body.append(card,rip,cur);
  let x=innerWidth*.62,y=innerHeight*.7;cur.style.transform=`translate(${x}px,${y}px)`;
  const ease=t=>t<.5?4*t*t*t:1-Math.pow(-2*t+2,3)/2;
  const sleep=ms=>new Promise(r=>setTimeout(r,ms));
  const D={
    sleep,
    async move(tx,ty,ms){
      const x0=x,y0=y,d=Math.hypot(tx-x0,ty-y0);ms=ms??Math.max(380,Math.min(1100,d*1.1));
      // A slight arc reads as a hand, not a robot.
      const bend=Math.min(60,d*.12),nx=-(ty-y0)/(d||1)*bend,ny=(tx-x0)/(d||1)*bend;
      const t0=performance.now();
      await new Promise(done=>{const f=now=>{const p=Math.min(1,(now-t0)/ms),e=ease(p),b=Math.sin(Math.PI*e);
        x=x0+(tx-x0)*e+nx*b*.5;y=y0+(ty-y0)*e+ny*b*.5;cur.style.transform=`translate(${x}px,${y}px)`;
        if(p<1)requestAnimationFrame(f);else done()};requestAnimationFrame(f)});
    },
    find(sel,text){
      const all=[...document.querySelectorAll(sel)].filter(e=>e.offsetParent!==null||getComputedStyle(e).position==='fixed');
      const el=text?all.find(e=>e.textContent.trim().includes(text)):all[0];
      if(!el)throw Error(`not found: ${sel} ${text??''}`);return el;
    },
    center(el){const r=el.getBoundingClientRect();return [r.left+r.width/2,r.top+r.height/2]},
    pulse(){rip.style.left=x+'px';rip.style.top=y+'px';rip.classList.remove('go');void rip.offsetWidth;rip.classList.add('go')},
    async hover(sel,text){const [cx,cy]=D.center(D.find(sel,text));await D.move(cx,cy)},
    async click(sel,text){
      const el=D.find(sel,text);const [cx,cy]=D.center(el);await D.move(cx,cy);await sleep(140);D.pulse();el.click();await sleep(180);
    },
    async choose(sel,value){
      const el=D.find(sel);const [cx,cy]=D.center(el);await D.move(cx,cy);await sleep(160);D.pulse();
      el.value=value;el.dispatchEvent(new Event('change',{bubbles:true}));await sleep(200);
    },
    async clickAt(cx,cy){
      await D.move(cx,cy);await sleep(160);D.pulse();
      const el=document.elementFromPoint(cx,cy);
      el?.dispatchEvent(new MouseEvent('click',{clientX:cx,clientY:cy,bubbles:true}));await sleep(160);
    },
    /** Centers of coral orbs on the multiverse canvas, found from its pixels. */
    coralOrbs(){
      const c=document.getElementById('field'),g=c.getContext('2d'),s=c.width/c.clientWidth;
      const img=g.getImageData(0,0,c.width,c.height).data,hits=[];
      for(let py=0;py<c.height;py+=2)for(let px=0;px<c.width;px+=2){const i=(py*c.width+px)*4;
        if(Math.abs(img[i]-255)<6&&Math.abs(img[i+1]-146)<10&&Math.abs(img[i+2]-138)<10&&img[i+3]>240)hits.push([px/s,py/s])}
      const clusters=[];
      for(const [hx,hy] of hits){const k=clusters.find(q=>Math.hypot(q.x-hx,q.y-hy)<14);if(k){k.n++;k.sx+=hx;k.sy+=hy;k.x=k.sx/k.n;k.y=k.sy/k.n}else clusters.push({x:hx,y:hy,sx:hx,sy:hy,n:1})}
      return clusters.filter(q=>q.n>=6).map(q=>[Math.round(q.x),Math.round(q.y)]);
    },
    async card({eyebrow,title,body,watch=[],foot,hold,keep=false}){
      const esc=s=>s.replace(/[&<>]/g,m=>({'&':'&amp;','<':'&lt;','>':'&gt;'}[m]));
      card.innerHTML=`<div class="in"><p class="eb">${esc(eyebrow)}</p><div class="rule"></div><h1>${title}</h1>`+
        (body?`<p class="body">${esc(body)}</p>`:'')+
        (watch.length?`<p class="wk">WHAT TO WATCH FOR</p><ol>${watch.map((w,i)=>`<li style="transition-delay:${.35+i*.22}s">${esc(w)}</li>`).join('')}</ol>`:'')+
        (foot?`<p class="foot">${esc(foot)}</p>`:'')+'</div>';
      cur.style.opacity='0';
      await sleep(30);card.classList.add('on');
      const words=(body+' '+watch.join(' ')).split(/\s+/).length;
      await sleep(hold??Math.max(5000,Math.min(10500,words*250+1500)));
      if(keep)return;
      card.classList.remove('on');await sleep(800);cur.style.opacity='1';
    },
    fps:0,
  };
  // A frame counter, to check the capture is smooth.
  let n=0,t0=performance.now();(function tick(t){n++;if(t-t0>=1000){D.fps=n;n=0;t0=t}requestAnimationFrame(tick)})(t0);
  window.__dir=D;
})();
