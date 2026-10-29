const fs=await import('node:fs/promises');
const extra=JSON.parse(await fs.readFile('D:/Project/acp-remote/.tmp/independent-jws-20261030/extra-vectors.json','utf8'));
const task=await taskSpace('independent-jws-20261030');
const result=await task.page('p1').evaluate(async proof=>{
 const jws=await import('/src/protocol/jws.ts');
 const f=await(await fetch('/tests/fixtures/jws-v1.json')).json();
 await jws.verifyCompactJws(proof,jws.decodeBase64urlStrict(f.clientPublicKey),['acpr-auth-client+jwt']);
 const key=await crypto.subtle.importKey('jwk',{...f.privateJwk,kty:'EC',crv:'P-256',ext:true,key_ops:['sign']},{name:'ECDSA',namedCurve:'P-256'},false,['sign']);
 const payload=JSON.stringify(JSON.parse(f.payload),null,2);
 const signed=await jws.signCompactJws('acpr-auth-client+jwt',new TextEncoder().encode(payload),key);
 return {userAgent:navigator.userAgent,secureContext:isSecureContext,rustWhitespaceHeaderAndPayloadVerified:true,signed};
},extra.originalBytesProof);
await fs.writeFile('D:/Project/acp-remote/.tmp/independent-jws-20261030/browser-pretty-proof.txt',result.signed);
delete result.signed;
await fs.writeFile('D:/Project/acp-remote/.tmp/independent-jws-20261030/raw-bytes.json',JSON.stringify(result,null,2));
console.log(result);
await task.finish({keep:[]});
