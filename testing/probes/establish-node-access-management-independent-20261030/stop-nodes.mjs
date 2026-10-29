import {load,stop,pipe,json} from './lib.mjs';
import {existsSync} from 'node:fs';
const s=await load('state.json');const result=[];
for(const n of [s.n,s.second]){const address=pipe(n.data);const pipePresent=existsSync(address);await stop(n);result.push({name:n.name,pid:n.pid,pipe:address,pipePresentBefore:pipePresent,pipeAbsentAfter:!existsSync(address),processExited:true,data:n.data});}
await json('cleanup-nodes.json',result);console.log(result.map(({name,pipePresentBefore,pipeAbsentAfter,processExited})=>({name,pipePresentBefore,pipeAbsentAfter,processExited})));
