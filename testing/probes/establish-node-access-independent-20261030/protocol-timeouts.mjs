import {writeFile} from 'node:fs/promises';
import {performance} from 'node:perf_hooks';
import {openControl,boundedNext,newIdentity} from './protocol-wire.mjs';
import {randomBytes} from 'node:crypto';
const root='D:/Project/acp-remote/.tmp/protocol-acceptance-20261030/';
async function test(mode){const ws=await openControl();const started=performance.now();let challengeAt=null;try{
  if(mode==='no-proof'){await new Promise(ok=>setTimeout(ok,4000));const identity=newIdentity();ws.send({v:1,type:'hello',clientId:null,clientPublicKey:identity.publicKey,clientNonce:randomBytes(32).toString('base64url')});const challenge=await boundedNext(ws,5000);if(challenge.message?.type!=='challenge')throw Error(JSON.stringify(challenge));challengeAt=performance.now()-started;}
  const result=await boundedNext(ws,18000);const elapsedMs=performance.now()-started;if(result.close?.code!==4408||result.close.reason!=='authentication timeout')throw Error(JSON.stringify(result));if(mode==='no-hello'&&(elapsedMs<4500||elapsedMs>7500))throw Error('nohello deadline outside bound');if(mode==='no-proof'&&(elapsedMs<13000||elapsedMs>17000))throw Error('total authentication bound unexpected');return {mode,tlsAuthorized:ws.authorized,elapsedMs,challengeAt,close:result.close,proofSent:false};
}finally{ws.destroy()}}
const results=await Promise.all([test('no-hello'),test('no-proof')]);await writeFile(root+'timeouts.json',JSON.stringify(results,null,2));console.log(results);
