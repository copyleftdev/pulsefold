// Juice: easing, a tiny synthesized sound palette (off by default), and motion preferences.
export const ease={
  outExpo:t=>t>=1?1:1-Math.pow(2,-10*t),
  outBack:t=>{const c=1.4;return 1+(c+1)*Math.pow(t-1,3)+c*Math.pow(t-1,2)},
  inOut:t=>t<.5?4*t*t*t:1-Math.pow(-2*t+2,3)/2,
};

export function prefersReducedMotion(){
  return typeof matchMedia==='function'&&matchMedia('(prefers-reduced-motion: reduce)').matches;
}

/** Short synthesized cues. Silent until enabled by a user gesture. */
export class Sound{
  constructor(){this.on=false;this.ctx=null}
  enable(on){
    this.on=on;
    if(on&&!this.ctx){const A=globalThis.AudioContext||globalThis.webkitAudioContext;if(A)this.ctx=new A()}
    if(on&&this.ctx?.state==='suspended')this.ctx.resume();
  }
  tone(freq,dur,{type='sine',gain=.04,at=0,slide=0}={}){
    if(!this.on||!this.ctx)return;
    const t=this.ctx.currentTime+at,o=this.ctx.createOscillator(),g=this.ctx.createGain();
    o.type=type;o.frequency.setValueAtTime(freq,t);
    if(slide)o.frequency.exponentialRampToValueAtTime(freq*slide,t+dur);
    g.gain.setValueAtTime(0,t);g.gain.linearRampToValueAtTime(gain,t+.008);g.gain.exponentialRampToValueAtTime(.0001,t+dur);
    o.connect(g).connect(this.ctx.destination);o.start(t);o.stop(t+dur+.02);
  }
  step(){this.tone(1320,.05,{gain:.018})}
  reject(){this.tone(330,.16,{type:'triangle',gain:.05,slide:.8})}
  fail(){this.tone(150,.38,{type:'sawtooth',gain:.03,slide:.7});this.tone(157,.38,{type:'sawtooth',gain:.022,slide:.68})}
  pass(){this.tone(784,.16,{gain:.03});this.tone(1046,.22,{gain:.025,at:.07})}
  found(){[523,659,523,415].forEach((f,i)=>this.tone(f,.12,{type:'triangle',gain:.035,at:i*.09}))}
  fixed(){[523,659,784,1046].forEach((f,i)=>this.tone(f,.2,{gain:.028,at:i*.07}))}
  tick(){this.tone(2400,.015,{gain:.006})}
}
