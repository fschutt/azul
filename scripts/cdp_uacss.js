#!/usr/bin/env node
// ua_css parity: read per-text-block rects from the azul-web page AND from
// Chrome rendering the same markup natively; print side by side.
const CDP_HTTP='http://127.0.0.1:9222';
const AZUL_URL='http://127.0.0.1:8800/';
const MARKUP='<!DOCTYPE html><html><body><h1>Heading one</h1><h2>Heading two</h2><p>First paragraph text.</p><p>Second paragraph text.</p><div>Plain div text.</div></body></html>';
const TEXTS=['Heading one','Heading two','First paragraph text.','Second paragraph text.','Plain div text.'];
function sleep(ms){return new Promise(r=>setTimeout(r,ms));}
async function drive(url, wait){
  const tab=await (await fetch(`${CDP_HTTP}/json/new?about:blank`,{method:'PUT'})).json();
  const ws=new WebSocket(tab.webSocketDebuggerUrl);
  let id=1;const pend=new Map();
  const send=(m,p={})=>new Promise((res,rej)=>{const i=id++;pend.set(i,{res,rej});ws.send(JSON.stringify({id:i,method:m,params:p}));});
  await new Promise((res,rej)=>{ws.onopen=res;ws.onerror=rej;});
  ws.onmessage=ev=>{const m=JSON.parse(ev.data);if(m.id&&pend.has(m.id)){const p=pend.get(m.id);pend.delete(m.id);m.error?p.rej(new Error(JSON.stringify(m.error))):p.res(m.result);}};
  await send('Runtime.enable');
  await send('Emulation.setDeviceMetricsOverride',{width:800,height:600,deviceScaleFactor:1,mobile:false});
  await send('Page.enable');await send('Page.navigate',{url});
  await sleep(wait);
  const expr=`(()=>{const wanted=${JSON.stringify(TEXTS)};const out={};
    const els=[...document.querySelectorAll('body *')];
    for(const t of wanted){
      const el=els.find(e=>e.childElementCount===0&&e.textContent.trim()===t)
             ||els.find(e=>e.textContent.includes(t)&&!els.some(c=>c!==e&&e.contains(c)&&c.textContent.includes(t)));
      if(el){const r=el.getBoundingClientRect();const cs=getComputedStyle(el);
        out[t]={x:+r.x.toFixed(1),y:+r.y.toFixed(1),w:+r.width.toFixed(1),h:+r.height.toFixed(1),fs:cs.fontSize,fw:cs.fontWeight};}
      else out[t]=null;
    } return out;})()`;
  const r=await send('Runtime.evaluate',{expression:expr,returnByValue:true});
  await fetch(`${CDP_HTTP}/json/close/${tab.id}`).catch(()=>{});
  return r.result.value;
}
async function main(){
  const azul=await drive(AZUL_URL, 9000);
  const chrome=await drive('data:text/html,'+encodeURIComponent(MARKUP), 1500);
  console.log('text                          | azul (x,y,w,h,fs)              | chrome (x,y,w,h,fs)');
  for(const t of TEXTS){
    const a=azul[t], c=chrome[t];
    const f=o=>o?`${o.x},${o.y} ${o.w}x${o.h} fs=${o.fs} fw=${o.fw}`:'NOT FOUND';
    console.log(`${t.padEnd(30)}| ${f(a).padEnd(31)}| ${f(c)}`);
  }
}
main().catch(e=>{console.error('failed:',e.message);process.exit(1);});
