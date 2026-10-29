import {root,load,delay,json} from './lib.mjs';
import {spawn} from 'node:child_process';
import fs from 'node:fs/promises';
const {n}=await load('state.json');
const child=spawn(root+'/bin/owner-pair-helper.exe',[n.data],{windowsHide:true,stdio:['pipe','pipe','pipe']});
let buffer='';let count=0;
child.stdout.on('data',async b=>{buffer+=b;let i;while((i=buffer.indexOf('\n'))>=0){const value=JSON.parse(buffer.slice(0,i));buffer=buffer.slice(i+1);if(count++===0)await json('browser-ticket.json',value);else await json('browser-approved.json',value)}});
const done=new Promise((ok,no)=>{child.once('error',no);child.once('exit',code=>code===0?ok():no(Error('browser pairing helper exit '+code)))});
const timeout=setTimeout(()=>child.kill(),240000);
try{for(let i=0;i<2300;i++){if(await fs.stat(root+'/approve-browser.flag').then(()=>true,()=>false)){child.stdin.end('approve\n');break}await delay(100)}await done;console.log('BROWSER_PAIR_APPROVED')}finally{clearTimeout(timeout);if(child.exitCode===null)child.kill()}
