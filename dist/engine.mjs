// Loads the Rust engine compiled to WebAssembly (no imports, no bindings) and exposes its
// text API: call(request) -> {code, out}. See engine/src/api.rs for the requests.
export const SEP='\u001e';
export async function load(source){
  const bytes=typeof source==='string'?await (await fetch(source)).arrayBuffer():source;
  const {instance}=await WebAssembly.instantiate(bytes,{});
  const x=instance.exports, enc=new TextEncoder(), dec=new TextDecoder();
  return {call(request){
    const b=enc.encode(request), p=x.pf_input(b.length);
    new Uint8Array(x.memory.buffer,p,b.length).set(b);
    const code=x.pf_call();
    const out=dec.decode(new Uint8Array(x.memory.buffer,x.pf_output_ptr(),x.pf_output_len()).slice());
    return {code,out};
  }};
}
