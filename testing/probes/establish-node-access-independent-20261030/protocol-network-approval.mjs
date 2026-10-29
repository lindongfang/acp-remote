import {writeFile} from 'node:fs/promises';
import {post,randomId,openControl,boundedNext} from './protocol-wire.mjs';
const results=[];
for(const path of ['/api/access/v1/pair/approve','/api/access/v1/device/approve']){const response=await post(path,{v:1,pairingId:randomId(),approved:true});if(response.status!==404)throw Error('unexpected approval endpoint '+JSON.stringify(response));results.push({path,status:response.status,error:response.body})}
const ws=await openControl();try{ws.send({v:1,type:'command',requestId:randomId(),action:'device.approve',payload:{pairingId:randomId()}});const result=await boundedNext(ws,5000);if(result.close?.code!==4400)throw Error('unauthenticated approval accepted');results.push({managementAttempt:result.close})}finally{ws.destroy()}
await writeFile('D:/Project/acp-remote/.tmp/protocol-acceptance-20261030/network-approval.json',JSON.stringify(results,null,2));console.log(results);
