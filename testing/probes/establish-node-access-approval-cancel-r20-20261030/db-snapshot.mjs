import fs from 'node:fs';import {createHash} from 'node:crypto';import {DatabaseSync} from 'node:sqlite';
const root='D:/Project/acp-remote/.tmp/approval-cancel-boundary-r20-20261030/',label=process.argv[2];
if(!/^[a-z0-9-]+$/.test(label??''))throw Error('snapshot label required');
const db=new DatabaseSync(root+'data-30455/node.sqlite3',{readOnly:true});
const clients=db.prepare('SELECT client_id,display_name,status,public_key,created_at,revoked_at FROM clients ORDER BY client_id').all().map(({public_key,...row})=>({...row,publicKeyFingerprint:createHash('sha256').update(public_key).digest('hex')}));
const audit=db.prepare('SELECT action,target_id,outcome FROM security_audit ORDER BY audit_id').all();db.close();
const value={label,at:new Date().toISOString(),clients,audit};fs.writeFileSync(root+'evidence/db-'+label+'.json',JSON.stringify(value,null,2));console.log(value);
