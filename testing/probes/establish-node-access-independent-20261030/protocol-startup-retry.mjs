import {spawn,execFile} from 'node:child_process';
import {promisify} from 'node:util';
import {mkdir,writeFile,chmod} from 'node:fs/promises';
import net from 'node:net';
const exec=promisify(execFile);
const root='D:/Project/acp-remote/.tmp/protocol-acceptance-20261030/';
const binary='D:/Project/acp-remote/.tmp/independent-acceptance-20261030/target/debug/acp-remote.exe';
const cert='D:/Project/acp-remote/.tmp/establish-node-access-tailscale-20261029/faraday.tail74842e.ts.net.crt';
const key='D:/Project/acp-remote/.tmp/establish-node-access-tailscale-20261029/faraday.tail74842e.ts.net.key';
async function config(name,bad=false){const dir=root+'startup-'+name;await mkdir(dir,{recursive:true});const data=dir+'/data';let values={data_dir:data,listen:'100.119.123.57:30452',canonical_origin:'https://faraday.tail74842e.ts.net:30452',tls_cert:cert,tls_key:key};if(bad&&name==='origin')values.canonical_origin='http://faraday.tail74842e.ts.net:30452';if(bad&&name==='tls'){await writeFile(dir+'/invalid-cert.pem','INDEPENDENT_INVALID_PUBLIC_CERTIFICATE');values.tls_cert=dir+'/invalid-cert.pem'}const path=dir+'/node.toml';await writeFile(path,Object.entries(values).map(([k,v])=>`${k} = ${JSON.stringify(v)}`).join('\n'));return{path,data,dir}}
function start(path){const child=spawn(binary,['--config',path,'daemon','start'],{windowsHide:true});let stdout='',stderr='';child.stdout.on('data',b=>stdout+=b);child.stderr.on('data',b=>stderr+=b);const done=new Promise((ok,no)=>{child.once('error',no);child.once('close',(code,signal)=>ok({code,signal,stdout,stderr}))});return{child,done,get stdout(){return stdout}}}
async function bounded(promise,ms){let timer;try{return await Promise.race([promise,new Promise((_,no)=>{timer=setTimeout(()=>no(Error('startup fixture timeout')),ms)})])}finally{clearTimeout(timer)}}
async function bind(){const server=net.createServer();await new Promise((ok,no)=>server.once('error',no).listen(30452,'100.119.123.57',ok));return server}
async function release(server){await new Promise(ok=>server.close(ok))}
async function healthy(path){const node=start(path);try{const deadline=Date.now()+10000;while(!node.stdout.includes('ACP Remote node ready')){if(node.child.exitCode!==null)throw Error('retry node exited');if(Date.now()>deadline)throw Error('retry not ready');await new Promise(ok=>setTimeout(ok,50))}const status=await exec(binary,['--config',path,'daemon','status'],{windowsHide:true,timeout:5000});const nodeId=/nodeId=([a-f0-9]+)/.exec(status.stdout)?.[1];if(!nodeId)throw Error('missing status identity');const stop=await exec(binary,['--config',path,'daemon','stop'],{windowsHide:true,timeout:10000});const end=await bounded(node.done,10000);if(end.code!==0||!stop.stdout.includes('state=stopped'))throw Error('retry stop failed');return{nodeId,stopExit:0,nodeExit:end.code}}finally{if(node.child.exitCode===null){try{await exec(binary,['--config',path,'daemon','stop'],{windowsHide:true,timeout:5000});await bounded(node.done,5000)}catch{node.child.kill()}}}}
const results=[];
for(const name of ['origin','tls','readonly-storage','occupied-port']){
 const fixture=await config(name,true);let blocker=null;
 try{
  if(name==='readonly-storage'){await mkdir(fixture.data,{recursive:true});await writeFile(fixture.data+'/node.sqlite3','');await chmod(fixture.data+'/node.sqlite3',0o444)}
  if(name==='occupied-port')blocker=await bind();
  const failed=start(fixture.path);let failure;try{failure=await bounded(failed.done,10000)}finally{if(failed.child.exitCode===null)failed.child.kill()}
  if(failure.code===0||failure.stdout.includes('ACP Remote node ready'))throw Error(name+' unexpectedly ready');
  if(failure.stderr.includes(key)||failure.stderr.includes('PRIVATE KEY'))throw Error(name+' leaked private material/path');
  if(blocker){await release(blocker);blocker=null}
  if(name==='readonly-storage')await chmod(fixture.data+'/node.sqlite3',0o666);
  await config(name,false);
  const retry=await healthy(fixture.path);const again=await healthy(fixture.path);if(retry.nodeId!==again.nodeId)throw Error('retry replaced identity');
  const rebound=await bind();await release(rebound);
  results.push({name,failure,retry,secondStart:again,portRebound:true});console.log({name,failureExit:failure.code,error:failure.stderr.trim(),retryExit:retry.nodeExit,sameNodeId:true,portRebound:true});
 }finally{if(blocker)await release(blocker);if(name==='readonly-storage')await chmod(fixture.data+'/node.sqlite3',0o666).catch(()=>{})}
}
await writeFile(root+'startup-retry.json',JSON.stringify(results,null,2));
