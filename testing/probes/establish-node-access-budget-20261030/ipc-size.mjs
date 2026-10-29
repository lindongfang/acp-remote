import net from 'node:net';
import fs from 'node:fs';
import {randomId} from './wire.mjs';
const root='.tmp/budget-acceptance-20261030/';const {pipe}=JSON.parse(fs.readFileSync(root+'pipe.json','utf8'));const results=[];
for(const bytes of [32767,32768,32769]){
 const requestId=randomId();const base=JSON.stringify({v:1,requestId,command:'status'});const body=base+' '.repeat(bytes-Buffer.byteLength(base));const start=performance.now();
 const actual=await new Promise((ok,no)=>{let text='';const s=net.connect(pipe);const timer=setTimeout(()=>{s.destroy();no(Error('IPC size observation timeout'))},5000);s.on('connect',()=>s.write(body+'\n'));s.on('data',c=>text+=c);s.on('error',no);s.on('close',()=>{clearTimeout(timer);let response=null;if(text.trim())response=JSON.parse(text.trim());ok({bodyBytes:bytes,wireBytes:bytes+1,responseReceived:response!==null,ok:response?.ok??null,requestMatched:response?.requestId===requestId,status:response?.status?.state??null,elapsedMs:Math.round(performance.now()-start)})})});
 const pass=bytes<=32768?actual.ok===true&&actual.requestMatched:!actual.responseReceived;results.push({actual,pass});fs.writeFileSync(root+'evidence/ipc-size.json',JSON.stringify(results,null,2));console.log(bytes,pass,JSON.stringify(actual));if(!pass)process.exitCode=1;
}
