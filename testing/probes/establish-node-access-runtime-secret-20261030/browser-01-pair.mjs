import fs from 'node:fs/promises';
const root='D:/Project/acp-remote/.tmp/runtime-secret-acceptance-20261030/';
const input=JSON.parse(await fs.readFile(root+'browser-input.json','utf8'));
const t=await taskSpace(11),p=t.page('p1');
await p.evaluate(()=>{
 const random=()=>Array.from(crypto.getRandomValues(new Uint8Array(24)),x=>x.toString(16).padStart(2,'0')).join('');
 window.__na014={secrets:[],network:[],results:[],echoes:[],markers:{proof:'NA014_PROOF_'+random(),private:'NA014_PRIVATE_'+random(),code:'NA014_CODE_'+random()}};
 window.__na014.secrets.push(...Object.values(window.__na014.markers));
 const hash=async s=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',new TextEncoder().encode(s))),x=>x.toString(16).padStart(2,'0')).join('');window.__na014.hash=hash;
 window.__na014.contains=s=>window.__na014.secrets.some(x=>String(s).includes(x));
 const original=window.fetch.bind(window);
 window.fetch=async(...args)=>{
  const body=typeof args[1]?.body==='string'?args[1].body:'';
  try{const v=JSON.parse(body);if(v.proof)window.__na014.secrets.push(v.proof);if(v.code)window.__na014.secrets.push(v.code);}catch{}
  const response=await original(...args);const text=await response.clone().text();
  const row={transport:'http',url:String(args[0]),method:args[1]?.method??'GET',requestBodySha256:await hash(body),status:response.status,responseEcho:window.__na014.contains(text),responseLength:text.length};
  if(response.status!==200){try{row.error=JSON.parse(text).error}catch{}}
  window.__na014.network.push(row);if(row.responseEcho)window.__na014.echoes.push({transport:'http',url:row.url});return response;
 };
 const NativeWebSocket=window.WebSocket;
 window.WebSocket=class extends NativeWebSocket{
  constructor(...args){super(...args);this.addEventListener('message',e=>{const echo=window.__na014.contains(e.data);window.__na014.network.push({transport:'wss-in',bytes:String(e.data).length,echo});if(echo)window.__na014.echoes.push({transport:'wss'});});this.addEventListener('close',e=>{window.__na014.network.push({transport:'wss-close',code:e.code,reason:e.reason,echo:window.__na014.contains(e.reason)});});}
  send(data){let m;try{m=JSON.parse(data);if(m.proof)window.__na014.secrets.push(m.proof);}catch{};window.__na014.network.push({transport:'wss-out',type:m?.type??'unknown',bytes:String(data).length,containsSyntheticMarker:Object.values(window.__na014.markers).some(x=>String(data).includes(x))});return super.send(data);}
 };
});
await p.fill('loc=css:input[name="pairing-code"]',input.code);
await p.fill('loc=css:input[name="display-name"]','NA014 isolated client');
await p.click('loc=role:button[name="认领这台浏览器"]');
await p.waitForFunction(()=>document.body.innerText.includes('等待本机'),undefined,{timeout:10000});
console.log({claimed:true});
