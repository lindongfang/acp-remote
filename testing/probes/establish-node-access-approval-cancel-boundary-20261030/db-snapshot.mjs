import fs from 'node:fs';import {DatabaseSync} from 'node:sqlite';
const root='D:/Project/acp-remote/.tmp/approval-cancel-boundary-20261030/',label=process.argv[2];
if(!/^[a-z0-9-]+$/.test(label??''))throw Error('snapshot label required');
const db=new DatabaseSync(root+'data-30455/node.sqlite3',{readOnly:true});
const clients=db.prepare('SELECT client_id,display_name,status,created_at,revoked_at FROM clients ORDER BY client_id').all();
const audit=db.prepare('SELECT action,target_id,outcome FROM security_audit ORDER BY audit_id').all();db.close();
const value={label,at:new Date().toISOString(),clients,audit};fs.writeFileSync(root+'evidence/db-'+label+'.json',JSON.stringify(value,null,2));console.log(value);
