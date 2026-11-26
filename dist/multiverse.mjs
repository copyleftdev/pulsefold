// Mission 04: hundreds of seeded universes at once. Every row comes from the engine's
// `universes` request; drilling in replays one universe in the flight recorder.
import {SEP} from './engine.mjs';
import {Field} from './field.mjs';

const $=id=>document.getElementById(id);
const el=(tag,text,cls)=>{const n=document.createElement(tag);if(text!==undefined)n.textContent=text;if(cls)n.className=cls;return n};

const SIM_VARIANTS=['stale-index','leftover-entry','closed-interval','wrong-bucket','index-before-check','no-self-exclusion','indexed','scan','rebuild'];
const RACE_VARIANTS=['torn-move','check-then-act','locked-writes','locked'];
const SIM_PROPS=['rejected_move','stale_write_observable','stale_write_hidden_from_named_probes','valid_move','move_after_rejection','touching_accept','duplicate_rejected','missing_move','chamber_full'];
const RACE_PROPS=['overlapping_ops','contended_write','conflict_under_contention','lock_contention','query_during_move'];
const SIM_KNOBS=[['ids','IDs in play','ids'],['move','move ‰','move'],['boundary','boundary ‰','boundary'],['horizon','horizon','horizon']];
const RACE_KNOBS=[['clients','clients','clients'],['ops','ops per client','ops'],['setup','setup reserves','setup'],['switch','switch ‰','switch'],['query','query ‰','query'],['move','move ‰','move']];
const OPS=48;

export function initMultiverse(game){
  const field=new Field($('field'),{onSelect:(u,i)=>detail(u),onHover:tip});
  const M={kind:'sim',variant:'stale-index',ran:null,running:false};
  game.onStill(on=>field.setReduced(on));

  const fillVariants=()=>{
    const list=M.kind==='sim'?SIM_VARIANTS:RACE_VARIANTS;
    if(!list.includes(M.variant))M.variant=list[0];
    $('mv-variant').replaceChildren(...list.map(v=>{const o=el('option',v);o.selected=v===M.variant;return o}));
  };
  const fillArrange=()=>{
    const knobs=M.kind==='sim'?SIM_KNOBS:RACE_KNOBS;
    $('mv-arrange').replaceChildren(el('option','seed order'),el('option','outcome'),...knobs.map(([k,label])=>{const o=el('option',label);o.value=k;return o}));
    $('mv-arrange').options[0].value='seed';$('mv-arrange').options[1].value='outcome';
  };
  const setKind=k=>{
    if(M.running)return;M.kind=k;
    $('mv-sim').setAttribute('aria-pressed',String(k==='sim'));$('mv-race').setAttribute('aria-pressed',String(k==='race'));
    fillVariants();fillArrange();
  };
  $('mv-sim').onclick=()=>setKind('sim');$('mv-race').onclick=()=>setKind('race');
  $('mv-variant').onchange=()=>{M.variant=$('mv-variant').value};
  $('mv-arrange').onchange=()=>{const o=$('mv-arrange').selectedOptions[0];field.arrange(o.value,o.textContent)};
  $('mv-failing').onclick=()=>{const on=$('mv-failing').getAttribute('aria-pressed')!=='true';$('mv-failing').setAttribute('aria-pressed',String(on));field.setFailingOnly(on)};
  setKind('sim');

  const margins=()=>{
    // The detail panel's column is always reserved on wide screens, so drilling in never moves an orb.
    const narrow=innerWidth<760;
    field.setMargins(narrow?{l:12,r:12,t:72,b:Math.round(innerHeight*.44)}:{l:300,r:348,t:84,b:28});
  };
  addEventListener('resize',margins);

  function parseRows(text){
    const [head,...rows]=text.trim().split('\n');const keys=head.split('\t');
    return rows.map(r=>{
      const c=Object.fromEntries(r.split('\t').map((v,i)=>[keys[i],v]));
      const props=new Set(c.properties==='-'?[]:c.properties.split(','));
      if(M.kind==='sim'){
        const failed=c.failed_at!=='-';
        const knobs={ids:+c.ids,move:+c.move,boundary:+c.boundary,horizon:+c.horizon};
        return {seed:+c.seed,failed,at:failed?+c.failed_at:null,check:c.check.replace('_',' '),checked:+c.checked,props,knobs,group:{...knobs,outcome:failed?'failing':'clean'}};
      }
      const failed=c.failure!=='-';
      const knobs={clients:+c.clients,ops:+c.ops,setup:+c.setup,switch:+c.switch,query:+c.query,move:+c.move};
      return {seed:+c.seed,failed,failure:c.failure.replace('_',' '),serial:c.serial,operations:+c.operations,props,knobs,group:{...knobs,outcome:failed?'failing':'clean'}};
    });
  }

  async function run(){
    if(M.running||!game.engine())return;
    M.running=true;$('mv-launch').disabled=true;$('mv-detail').hidden=true;margins();
    const kind=M.kind,variant=M.variant,count=+$('mv-count').value,props=kind==='sim'?SIM_PROPS:RACE_PROPS;
    M.ran={kind,variant,count,props,list:[]};
    field.clear(count);
    const o=$('mv-arrange').selectedOptions[0];field.arrange(o.value,o.textContent);
    try{
      for(let first=1;first<=count;first+=25){
        const n=Math.min(25,count-first+1);
        const req=kind==='sim'?`universes sim ${variant} ${first} ${n} ${OPS}`:`universes race ${variant} ${first} ${n}`;
        const {code,out}=game.engine().call(req);
        if(code!==0)throw Error(out);
        const rows=parseRows(out);M.ran.list.push(...rows);field.add(rows,props);
        stats();game.sound.tick();
        await new Promise(r=>requestAnimationFrame(()=>r()));
      }
      const k=M.ran.list.filter(u=>u.failed).length;
      if(k)game.banner(`${k} OF ${count} UNIVERSES BROKE`,`${variant.toUpperCase()} · CLICK ONE TO DRILL IN`);
      else game.banner('EVERY UNIVERSE HELD',`${count} SEEDS · ${variant.toUpperCase()}`,'mint');
      k?game.sound.found():game.sound.pass();
    }catch(err){game.banner('ENGINE ERROR',String(err.message).slice(0,80))}
    finally{M.running=false;$('mv-launch').disabled=false}
  }
  $('mv-launch').onclick=run;

  function stats(){
    const r=M.ran,n=r.list.length,k=r.list.filter(u=>u.failed).length;
    const box=$('mv-stats');box.replaceChildren();
    const big=el('b',`${k} / ${n}`,k?'coral':'');box.append(big,el('span',`universes where ${r.variant} broke`));
    if(r.kind==='race'){const s=r.list.filter(u=>u.serial==='fail').length;box.append(el('br'),el('span',`${s} fail with no preemption inside an operation`))}
    const cov=$('mv-coverage');cov.replaceChildren(el('span','PROPERTIES REACHED · UNIVERSES','cov-h'));
    for(const p of r.props){
      const m=r.list.filter(u=>u.props.has(p)).length,row=el('div',undefined,'cov');
      const bar=el('i');bar.style.setProperty('--w',`${n?m/n*100:0}%`);
      row.append(el('span',p.replaceAll('_',' ')),el('span',`${m}`),bar);cov.append(row);
    }
  }

  function tip(u,e){
    const t=$('mv-tip');
    if(!u){t.hidden=true;return}
    t.replaceChildren(el('span',`seed ${u.seed} · `),el('span',u.failed?'broke':'held',u.failed?'coral':''),el('span',` · ${Object.entries(u.knobs).map(([k,v])=>`${k} ${v}`).join(' · ')}`));
    t.hidden=false;
    if(e){const x=Math.min(innerWidth-t.offsetWidth-8,e.clientX+14),y=Math.max(8,e.clientY-36);t.style.left=x+'px';t.style.top=y+'px'}
  }

  function detail(u){
    const d=$('mv-detail');
    if(!u){d.hidden=true;margins();return}
    const r=M.ran,knobs=r.kind==='sim'?SIM_KNOBS:RACE_KNOBS;
    const out=r.kind==='sim'
      ?(u.failed?`× Broke at operation ${u.at} of ${OPS}, caught by the ${u.check}`:`✓ Held for all ${u.checked} operations`)
      :(u.failed?`× ${u.failure[0].toUpperCase()+u.failure.slice(1)} (${u.operations} operations)`:`✓ Linearizable (${u.operations} operations)`);
    const dl=el('dl');for(const [k,label] of knobs)dl.append(el('dt',label),el('dd',String(u.knobs[k])));
    dl.append(el('dt','implementation'),el('dd',r.variant));
    if(r.kind==='race')dl.append(el('dt','serial run'),el('dd',u.serial==='fail'?'fails':'passes'));
    const ul=el('ul');for(const p of r.props){const li=el('li',p.replaceAll('_',' '));if(u.props.has(p))li.className='on';ul.append(li)}
    const replay=el('button','Replay in recorder','primary');replay.onclick=()=>open(u);
    const close=el('button','Close','ghost');close.onclick=()=>field.select(-1);
    const acts=el('div',undefined,'acts');acts.append(replay,close);
    d.replaceChildren(el('p','UNIVERSE','k'),el('h2',`Seed ${u.seed}`),el('p',out,'out '+(u.failed?'coral':'mint')),
      el('p','SWARM KNOBS','k'),dl,el('p',`PROPERTIES REACHED · ${u.props.size}/${r.props.length}`,'k'),ul,acts);
    d.hidden=false;margins();
  }

  function open(u){
    const r=M.ran;
    if(r.kind==='sim'){
      const {code,out}=game.engine().call(`sim ${r.variant} ${u.seed} ${OPS} 1`);
      if(code===2)return game.banner('ENGINE ERROR',out);
      game.playTrace(out.split(SEP)[1],{meta:`universe ${u.seed} · ${r.variant}${u.failed?' · shrunk':''}`,back:show});
    }else{
      const {code,out}=game.engine().call(`race ${r.variant} ${u.seed} 1`);
      if(code===2)return game.banner('ENGINE ERROR',out);
      game.playRace(out.split(SEP)[1],u.seed,{back:show});
    }
  }

  function show(){
    game.showScreen('multiverse');
    $('field').hidden=false;
    if(field.selected>=0)detail(field.orbs[field.selected]);
    margins();field.request();
  }

  // Arrow keys move between universes in reading order; Enter replays.
  addEventListener('keydown',e=>{
    if($('field').hidden||!field.orbs.length||e.target instanceof Element&&e.target.closest('select,input'))return;
    const n=field.orbs.length,i=field.selected;
    // Move to the nearest universe on screen in the arrow's direction, across groups too.
    const dirs={ArrowRight:[1,0],ArrowLeft:[-1,0],ArrowDown:[0,1],ArrowUp:[0,-1]};
    if(dirs[e.key]){
      e.preventDefault();
      if(i<0){let first=0;field.orbs.forEach((p,j)=>{const q=field.orbs[first];if(p.ty<q.ty-1||(Math.abs(p.ty-q.ty)<1&&p.tx<q.tx))first=j});field.select(first);return}
      const o=field.orbs[i],[dx,dy]=dirs[e.key];
      let best=-1,bd=Infinity;
      field.orbs.forEach((p,j)=>{
        const along=(p.tx-o.tx)*dx+(p.ty-o.ty)*dy;if(along<=field.cell*.5)return;
        const across=Math.abs((p.tx-o.tx)*dy)+Math.abs((p.ty-o.ty)*dx);
        const d=along+across*3;if(d<bd){bd=d;best=j}
      });
      if(best>=0)field.select(best);return;
    }
    if(e.key==='Enter'&&i>=0){e.preventDefault();open(field.orbs[i])}
  });

  return {show,start(){fillVariants();show()}};
}
