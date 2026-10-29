import https from 'node:https';
import fs from 'node:fs';
import {openControl} from './wire.mjs';
const out=[];const root='.tmp/budget-acceptance-20261030/evidence/';
function save(id,actual,pass){out.push({id,actual,pass});fs.writeFileSync(root+'global.json',JSON.stringify(out,null,2));console.log(id,pass,JSON.stringify(actual));}
function post(endpoint,source){return new Promise((ok,no)=>{const req=https.request({host:'127.0.0.1',port:30458,servername:'faraday.tail74842e.ts.net',rejectUnauthorized:true,localAddress:source,method:'POST',path:'/api/access/v1/pair/'+endpoint,headers:{Host:'faraday.tail74842e.ts.net:30458',Origin:'https://faraday.tail74842e.ts.net:30458','Content-Type':'application/json','Content-Length':1}},res=>{let b='';res.on('data',c=>b+=c);res.on('end',()=>ok({status:res.statusCode,body:JSON.parse(b),retry:res.headers['retry-after']}))});req.on('error',no);req.end('{')})}
for(const [ep,per,total] of [['lookup',5,30],['claim',30,120],['challenge',30,120],['status',30,120]]){const replies=[];for(let i=0;i<=total;i++)replies.push(await post(ep,`127.0.0.${10+Math.floor(i/per)}`));save('global-'+ep,{count:replies.length,counts:replies.reduce((a,r)=>(a[r.status]=(a[r.status]??0)+1,a),{}),last:replies.at(-1)},replies.slice(0,total).every(r=>r.status===400)&&replies.at(-1).status===429)}
const hs=[];for(let i=0;i<121;i++){try{const ws=await openControl(`127.0.0.${30+Math.floor(i/30)}`,30458);hs.push(101);ws.destroy()}catch(e){hs.push(e.message)}}save('global-handshake',{accepted:hs.filter(x=>x===101).length,last:hs.at(-1)},hs.slice(0,120).every(x=>x===101)&&String(hs.at(-1)).includes('429'));
// Lookup was globally exhausted above; capture the retry deadline and retry
// the same real source after it. Wall time is measured, not simulated.
const start=performance.now();const denied=await post('lookup','127.0.0.10');const wait=Number(denied.retry);await new Promise(ok=>setTimeout(ok,(wait+0.1)*1000));const after=await post('lookup','127.0.0.10');const elapsedMs=Math.round(performance.now()-start);save('fixed-window-real',{denied,waitSeconds:wait,elapsedMs,after},denied.status===429&&after.status===400);
