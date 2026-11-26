// Trace v2 parser. The Rust runner owns every verdict; this only validates and reads rows.
export const header=['turn','step','variant','operation','expected','result','ledger','index',
  'original_window','reference_original','implementation_original',
  'attempted_window','reference_attempted','implementation_attempted',
  'marker','refactor','verdict','reason'].join('\t');
const slot=/^([a-z0-9_]+)@C([12]):(\d+)-(\d+)$/i;
const OUTCOME=/^(OK|CONFLICT|DUPLICATE|NOT_FOUND)$/, ROOMS=/^(C1,C2|C1|C2|none)$/;
const VARIANTS=['scan','indexed','stale-index','rebuild'], REFACTOR=['used','not_used','no_evidence','n/a'];
const MARKERS=['EXACT','ABSENT','MALFORMED','UNOBSERVED'];
const span=s=>{
  const m=/^(\d+)-(\d+)$/.exec(s??'');
  if(!m||+m[1]>=+m[2]||+m[2]>65535) throw Error('Invalid window.');
  return {lo:+m[1],hi:+m[2]};
};
export function slots(s){
  if(s==='none') return [];
  return (s??'').split(' ').map(v=>{
    const a=v.match(slot);
    if(!a || +a[3]>=+a[4] || +a[4]>65535) throw Error('Invalid chamber slot.');
    return {id:a[1],room:+a[2],lo:+a[3],hi:+a[4]};
  });
}
function probe(window,ref,impl,optional){
  if(optional&&window==='-'&&ref==='-'&&impl==='-') return null;
  if(!ROOMS.test(ref??'')||!ROOMS.test(impl??'')) throw Error('Invalid availability.');
  return {...span(window),ref,impl};
}
export function parse(raw){
  const lines=raw.trimEnd().split(/\r?\n/);
  if(lines[0]!==header||lines.length<2||lines.length>10001) throw Error('Expected a Pulsefold trace v2 TSV with 1–10,000 steps.');
  let turn=1;
  return lines.slice(1).map((l,i)=>{
    const f=l.split('\t');
    const [t,n,variant,op,expected,result,ledger,index,ow,ro,io,aw,ra,ia,marker,refactor,verdict,reason]=f;
    try{
      if(f.length!==18 || n!==String(i+1) || !/^[1-9]\d{0,3}$/.test(t??'') || +t<turn || +t>turn+1 || (i===0&&t!=='1') ||
        !VARIANTS.includes(variant) || !/^(reserve [a-z0-9_]{1,40} [01] \d{1,5} \d{1,5}|move [a-z0-9_]{1,40} \d{1,5} \d{1,5})$/i.test(op??'') ||
        !OUTCOME.test(expected??'') || !OUTCOME.test(result??'') || !MARKERS.includes(marker) || !REFACTOR.includes(refactor) ||
        !/^(PASS|FAIL)$/.test(verdict??'') || (verdict==='PASS')!==(reason==='-') || !/^[\x20-\x7e]{1,500}$/.test(reason??''))
        throw Error();
      turn=+t;
      return {turn,n:+n,variant,op,expected,result,ledger:slots(ledger),index:slots(index),
        original:probe(ow,ro,io,true),attempted:probe(aw,ra,ia,false),marker,refactor,verdict,reason};
    }catch{throw Error('Invalid replay row '+(i+1)+'.')}
  });
}
export function firstFailure(rows){return rows.findIndex(r=>r.verdict==='FAIL')}
export function seek(rows,index){return Math.max(0,Math.min(rows.length-1,index))}
export function domain(rows) {
  let min=600,max=840;
  for(const row of rows){
    for(const side of [row.ledger,row.index])for(const item of side){min=Math.min(min,item.lo);max=Math.max(max,item.hi)}
    for(const p of [row.original,row.attempted]) if(p){min=Math.min(min,p.lo);max=Math.max(max,p.hi)}
  }
  return [min,max];
}
