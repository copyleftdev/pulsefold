// A virtual clock, installed before the page's own scripts run. The director advances it one
// frame at a time and screenshots each frame, so the video is a perfect 60 fps however long
// rendering takes. Covers performance.now, Date.now, timers, requestAnimationFrame, and CSS
// animations and transitions (through the Web Animations API). Media only.
(()=>{
  let now=0,nextId=1,inflight=0;
  // Fixed epoch and seeded randomness, so every Chrome instance renders identical frames.
  const epoch=1764115200000,timers=new Map(),rafs=new Map(),starts=new WeakMap();
  const realTimeout=window.setTimeout.bind(window);
  let seed=0x9e3779b9;
  Math.random=()=>{seed=(seed+0x6D2B79F5)>>>0;let t=seed;t=Math.imul(t^t>>>15,t|1);t^=t+Math.imul(t^t>>>7,t|61);return((t^t>>>14)>>>0)/4294967296};
  // The clock waits for network requests, body reads and WebAssembly compiles, so none of
  // them can take a different number of frames from one run to the next.
  const track=p=>{inflight++;return Promise.resolve(p).finally(()=>{inflight--})};
  for(const m of ['text','arrayBuffer','json','blob']){const f=Response.prototype[m];Response.prototype[m]=function(...a){return track(f.apply(this,a))}}
  const realFetch=window.fetch.bind(window);
  window.fetch=(...a)=>track(realFetch(...a));
  for(const m of ['instantiate','compile','instantiateStreaming']){const f=WebAssembly[m];if(f)WebAssembly[m]=(...a)=>track(f.apply(WebAssembly,a))}
  performance.now=()=>now;
  Date.now=()=>epoch+Math.round(now);
  window.setTimeout=(fn,ms=0,...a)=>{const id=nextId++;timers.set(id,{fn:()=>typeof fn==='function'&&fn(...a),at:now+Math.max(0,+ms||0),every:0});return id};
  window.setInterval=(fn,ms=0,...a)=>{const every=Math.max(1,+ms||0),id=nextId++;timers.set(id,{fn:()=>fn(...a),at:now+every,every});return id};
  window.clearTimeout=window.clearInterval=id=>{timers.delete(id)};
  window.requestAnimationFrame=fn=>{const id=nextId++;rafs.set(id,fn);return id};
  window.cancelAnimationFrame=id=>{rafs.delete(id)};
  const settle=()=>new Promise(r=>queueMicrotask(r));
  window.__vt={
    get now(){return now},
    /** Advances the clock by `dt` ms: due timers in order, CSS animations, then one animation frame. */
    async tick(dt){
      while(inflight>0)await new Promise(r=>realTimeout(r,5));
      const target=now+dt;
      for(;;){
        let due=null;
        for(const [id,t] of timers)if(t.at<=target&&(!due||t.at<due[1].at))due=[id,t];
        if(!due)break;
        const [id,t]=due;now=Math.max(now,t.at);
        if(t.every)t.at+=t.every;else timers.delete(id);
        try{t.fn()}catch(e){console.error(e)}
        for(let i=0;i<8;i++)await settle();
      }
      now=target;
      for(const a of document.getAnimations()){
        if(!starts.has(a))starts.set(a,now-dt);
        if(a.playState!=='paused')a.pause();
        a.currentTime=now-starts.get(a);
      }
      const cbs=[...rafs.values()];rafs.clear();
      for(const cb of cbs){try{cb(now)}catch(e){console.error(e)}}
      for(let i=0;i<8;i++)await settle();
      return now;
    },
  };
})();
