import fs from 'node:fs/promises';
const root='D:/Project/acp-remote/.tmp/runtime-secret-acceptance-20261030/';
const t=await taskSpace(11),p=t.page('p1');
const output=await p.evaluate(async()=>{
 const a=window.__na014,r=a.originalIdentity;
 await a.probe('revoked',r.clientId,'marked');
 await a.probe('revoked',r.clientId,'wrong-signature');
 await a.probe('revoked-well-signed',r.clientId,'well-signed');
 const randomId=btoa(String.fromCharCode(...crypto.getRandomValues(new Uint8Array(16)))).replace(/=/g,'').replace(/\+/g,'-').replace(/\//g,'_');
 const response=await fetch('/api/access/v1/pair/lookup',{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify({v:1,code:'ZZZZ-ZZZZ',requestId:randomId}),cache:'no-store'});
 a.httpResults.push({path:'lookup-valid-schema-unknown-code',status:response.status,body:await response.json()});
 const uniform=a.results.every(x=>x.code===4401&&x.reason==='authentication failed'&&x.messages.join(',')==='challenge'&&!x.echo);
 return {results:a.results,http:a.httpResults,uniform,echoes:a.echoes,previousAuthenticatedConnectionRevoked:a.network.some(x=>x.transport==='wss-close'&&x.code===4410),privateExtractable:r.privateKey.extractable};
});
await fs.writeFile(root+'evidence/authentication-results.json',JSON.stringify(output,null,2));
console.log(output);
if(!output.uniform||output.echoes.length||output.privateExtractable)throw Error('NA014 authentication or echo assertion failed');
