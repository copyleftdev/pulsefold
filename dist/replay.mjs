export const header='step\toperation\tresult\tledger\tindex\toracle_old\tindex_old\toracle_new\tindex_new\tcanary\tverdict';
const slot=/^([a-z0-9_]+)@C([12]):(\d+)-(\d+)$/i;
export function slots(s){
  if(!s) return [];
  return s.split(' ').map(v=>{
    const a=v.match(slot);
    if(!a || +a[3]>=+a[4] || +a[4]>65535) throw Error('Invalid chamber slot.');
    return {id:a[1],room:+a[2],lo:+a[3],hi:+a[4]};
  });
}
export function parse(raw){
  const lines=raw.trimEnd().split(/\r?\n/);
  if(lines[0]!==header||lines.length<2||lines.length>10001) throw Error('Expected a Pulsefold TSV with 1–10,000 steps.');
  return lines.slice(1).map((l,i)=>{
    const [n,op,result,ledger,index,oldL,oldI,newL,newI,marker,verdict,...extra]=l.split('\t');
    if(extra.length || n!==String(i+1) || !/^[a-z0-9_ ]{1,100}$/i.test(op??'') ||
      !/^(OK|CONFLICT|DUPLICATE|NOT_FOUND)$/.test(result??'') ||
      ![oldL,oldI,newL,newI].every(x=>/^(C1,C2|C1|C2)?$/.test(x??'!')) ||
      !/^(EXACT|ABSENT|MALFORMED|UNOBSERVED)$/.test(marker??'') || !/^(PASS|FAIL)$/.test(verdict??''))
      throw Error('Invalid replay row '+(i+1)+'.');
    return {n:+n,op,result,ledger:slots(ledger),index:slots(index),oldL,oldI,newL,newI,marker,verdict};
  });
}
export function firstFailure(rows){return rows.findIndex(r=>r.verdict==='FAIL')}
export function seek(rows,index){return Math.max(0,Math.min(rows.length-1,index))}
export function domain(rows) {
  let min=600,max=840;
  for(const row of rows) for(const side of [row.ledger,row.index])for(const item of side){min=Math.min(min,item.lo);max=Math.max(max,item.hi)}
  return [min,max];
}
