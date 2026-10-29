import {root,load,stop,start,json,db,assert} from './lib.mjs';
import fs from 'node:fs/promises';
const s=await load('state.json');
await stop(s.n);await stop(s.second);
await fs.copyFile('D:/Project/acp-remote/.tmp/acceptance-20261030-r5-target/debug/acp-remote.exe',root+'/bin/acp-remote.exe');
s.n=await start('node-one',30451);s.second=await start('node-two',30452);
await json('state.json',s);
const d=db(s.n);assert(d.prepare('SELECT count(*) n FROM clients').get().n===70,'R17 retains70 records including genuine Web identity');d.close();
console.log('R17_READY');
