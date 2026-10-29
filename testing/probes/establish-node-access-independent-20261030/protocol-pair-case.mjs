import {readFile,writeFile} from 'node:fs/promises';
import {randomBytes} from 'node:crypto';
import {origin,newIdentity,compact,verified,post,randomId,openControl,boundedNext} from './protocol-wire.mjs';
const root='D:/Project/acp-remote/.tmp/protocol-acceptance-20261030/';
const input=JSON.parse(await readFile(root+'pair-input.json','utf8'));
const identity=newIdentity();
const lookup=await post('/api/access/v1/pair/lookup',{v:1,code:input.code,requestId:randomId()});
if(lookup.status!==200)throw Error('lookup failed '+JSON.stringify(lookup));
const l=lookup.body;
const payload={v:l.v,nodeId:l.nodeId,origin:l.origin,requestId:l.requestId,challengeId:l.challengeId,nonce:l.nonce,iat:l.iat,exp:l.exp,pairingId:l.pairingId,clientPublicKey:identity.publicKey,displayName:'Independent NA020 '+input.mode};
const claim=await post('/api/access/v1/pair/claim',{v:1,proof:compact('acpr-pair-claim+jwt',payload,identity.privateKey)});
if(claim.status!==200)throw Error('claim rejected '+JSON.stringify(claim));
const initial=verified(claim.body.hostProof,l.nodePublicKey,'acpr-host-claim+jwt');
if(initial.status!=='pending_confirmation')throw Error('unexpected initial state');
console.log('CLAIM_READY mode='+input.mode+' status=pending_confirmation nodeId='+l.nodeId);
let status=null;const deadline=Date.now()+120000;
while(Date.now()<deadline){
  const c=await post('/api/access/v1/pair/challenge',{v:1,pairingId:l.pairingId,clientPublicKey:identity.publicKey,requestId:randomId()});
  if(c.status!==200)throw Error('status challenge rejected '+JSON.stringify(c));
  const b=c.body;
  const statusPayload={v:b.v,nodeId:b.nodeId,origin:b.origin,requestId:b.requestId,challengeId:b.challengeId,nonce:b.nonce,iat:b.iat,exp:b.exp,pairingId:l.pairingId,clientPublicKey:identity.publicKey};
  const r=await post('/api/access/v1/pair/status',{v:1,proof:compact('acpr-pair-status+jwt',statusPayload,identity.privateKey)});
  if(r.status!==200)throw Error('status rejected '+JSON.stringify(r));
  status=verified(r.body.hostProof,l.nodePublicKey,'acpr-host-status+jwt');
  if(status.status!=='pending_confirmation')break;
  await new Promise(ok=>setTimeout(ok,3000));
}
const expected=['reject','enter'].includes(input.mode)?'rejected':'cancelled';
if(status?.status!==expected||status.clientId!==null)throw Error('terminal mismatch '+JSON.stringify(status));
const ws=await openControl();let management;
try{ws.send({v:1,type:'hello',clientId:null,clientPublicKey:identity.publicKey,clientNonce:randomBytes(32).toString('base64url')});const c=await boundedNext(ws,5000);if(c.message?.type!=='challenge')throw Error('missing auth challenge');const host=verified(c.message.hostProof,l.nodePublicKey,'acpr-auth-host+jwt');if(host.origin!==origin||host.clientPublicKey!==identity.publicKey)throw Error('auth binding mismatch');ws.send({v:1,type:'proof',proof:compact('acpr-auth-client+jwt',host,identity.privateKey)});management=await boundedNext(ws,5000);if(management.close?.code!==4401)throw Error('cancelled identity authenticated '+JSON.stringify(management));}finally{ws.destroy()}
const result={mode:input.mode,pendingObserved:true,terminal:status.status,clientId:status.clientId,managementClose:management.close,tlsValidation:true,privateKeyPersisted:false};
await writeFile(root+input.mode+'.json',JSON.stringify(result,null,2));console.log(result);
