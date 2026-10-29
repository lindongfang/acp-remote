import tls from 'node:tls';
import fs from 'node:fs/promises';
const results=[];
for(const port of [30455,30456]){
 const result=await new Promise((ok,no)=>{const s=tls.connect({host:'faraday.tail74842e.ts.net',port,servername:'faraday.tail74842e.ts.net',rejectUnauthorized:true},()=>{const c=s.getPeerCertificate();ok({port,authorized:s.authorized,protocol:s.getProtocol(),certificateFingerprint256:c.fingerprint256,subject:c.subject,issuer:c.issuer,validTo:c.valid_to});s.destroy();});s.setTimeout(7000,()=>s.destroy(Error('TLS deadline')));s.on('error',no);});results.push(result);
}
await fs.writeFile('D:/Project/acp-remote/.tmp/approval-response-loss-20261030/evidence/tls.json',JSON.stringify(results,null,2));console.log(results);
