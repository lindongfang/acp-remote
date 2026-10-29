import {load,db,assert,json,root} from './lib.mjs';
import fs from 'node:fs/promises';
const {n}=await load('state.json');const d=db(n);try{
 const audits=d.prepare('SELECT * FROM security_audit ORDER BY audit_id').all();
 const expected=['action','actor_id','audit_id','occurred_at','outcome','target_id'].sort().join('/');
 assert(audits.every(row=>Object.keys(row).sort().join('/')===expected),'audit rows have only fixed event metadata fields');
 assert(audits.every(r=>['device.pair.approve','device.revoke'].includes(r.action)),'audits only approval/revocation facts, not per frame');
 const ticket=await load('browser-ticket.json');const markers=[ticket.code,ticket.code.replace('-','')];
 const logs=await fs.readFile(root+'/node-one/node.log','utf8');const text=JSON.stringify(audits);
 assert(markers.every(m=>!logs.includes(m)&&!text.includes(m)),'node log/audit do not contain actual test pairing code');
 const commands=d.prepare('SELECT result_json,payload_digest FROM management_commands').all();
 assert(commands.every(r=>{const v=JSON.parse(r.result_json);return Object.keys(v).sort().join('/')==='clientId/status'&&v.status==='revoked'&&/^[a-f0-9]{64}$/.test(r.payload_digest)}),'stored revoke results contain only closed result fields/digest');
 await json('audit-output.json',{auditRows:audits.length,actions:audits.reduce((out,r)=>(out[r.action]=(out[r.action]??0)+1,out),{}),commandRows:commands.length,metadataFields:expected.split('/'),noCodeInAuditOrLogs:true,closedResults:true,finalRevision:d.prepare('SELECT revision FROM node_state').get().revision});
}finally{d.close()}
