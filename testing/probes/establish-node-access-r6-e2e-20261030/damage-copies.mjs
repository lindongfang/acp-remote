import {DatabaseSync} from 'node:sqlite';
const fs=await import('node:fs/promises');const root=process.env.ACP_E2E_ROOT??'D:/Project/acp-remote/.tmp/r6-e2e-acceptance-20261030';
const action=process.argv[2];
if(action==='prepare'){
 const template=await fs.readFile(`${root}/node.toml`,'utf8');
 for(const name of ['key-corrupt','key-unreadable','meta-mismatch']){
  await fs.cp(`${root}/data`,`${root}/damage/${name}`,{recursive:true,errorOnExist:true,force:false});
  await fs.writeFile(`${root}/damage/${name}.toml`,template.replace(`${root}/data`,`${root}/damage/${name}`).replaceAll('30453','30454'));
 }
 await fs.writeFile(`${root}/damage/key-corrupt/identity.key`,Buffer.from('isolated invalid ciphertext'));
 const db=new DatabaseSync(`${root}/damage/meta-mismatch/node.sqlite3`);db.prepare('UPDATE node_meta SET node_id=?').run('f'.repeat(64));db.close();
}
const names=['original','key-corrupt','key-unreadable','meta-mismatch'];const states={};
for(const name of names){const dir=name==='original'?`${root}/data`:`${root}/damage/${name}`;const db=new DatabaseSync(`${dir}/node.sqlite3`,{readOnly:true});states[name]={meta:db.prepare('SELECT schema_version,node_id,key_file_ref FROM node_meta').get(),clients:db.prepare('SELECT client_id,status FROM clients ORDER BY client_id').all(),auditCount:db.prepare('SELECT COUNT(*) AS n FROM security_audit').get().n};db.close()}
await fs.writeFile(`${root}/evidence/damage-${action}.json`,JSON.stringify(states,null,2));console.log({action,states});
