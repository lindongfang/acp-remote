import fs from 'node:fs';
import {DatabaseSync} from 'node:sqlite';
const root='D:/Project/acp-remote/.tmp/runtime-secret-acceptance-20261030/';
const db=new DatabaseSync(root+'data-30455/node.sqlite3',{readOnly:true});
const rows=db.prepare('SELECT * FROM security_audit ORDER BY audit_id').all();
const summary={rows:rows.length,actions:rows.map(r=>({action:r.action,outcome:r.outcome,actorId:r.actor_id,targetId:r.target_id})),columns:db.prepare('PRAGMA table_info(security_audit)').all().map(r=>r.name)};
// Raw rows remain inside the private runtime only until the page checks all in-memory candidates.
fs.writeFileSync(root+'audit-transient.json',JSON.stringify(rows));
fs.writeFileSync(root+'evidence/audit-summary.json',JSON.stringify(summary,null,2));
db.close();console.log({auditRows:summary.rows,actions:summary.actions.map(r=>r.action)});
