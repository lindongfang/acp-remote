import fs from 'node:fs/promises';
import {connect,root} from './edge-cdp.mjs';
const phase=process.argv[2],c=await connect();
try{
 if(phase==='init'){
  await c.send('Page.enable',{},c.sessionId);await c.send('Page.navigate',{url:'https://faraday.tail74842e.ts.net:30455/'},c.sessionId);
  for(let i=0;i<30;i++){if(await c.evaluate('!!document.querySelector("input[name=pairing-code]")'))break;await new Promise(ok=>setTimeout(ok,100));}
  const isolation=await c.evaluate('({origin:location.origin,seenOtherSentinel:localStorage.getItem("na021-profile-sentinel"),inputNames:Array.from(document.querySelectorAll("input")).map(x=>x.name)})');
  if(isolation.seenOtherSentinel!==null)throw Error('Edge profile not isolated');
  await c.evaluate('localStorage.setItem("na021-profile-sentinel","owned-edge-profile-B")');
  const installed=await c.evaluate(await fs.readFile(new URL('./harness.js',import.meta.url),'utf8'));console.log({isolation,installed});
 }else if(phase==='claim'){
  const {code}=JSON.parse(await fs.readFile(root+'browser-input.json','utf8'));
  await c.evaluate(`(()=>{document.querySelector('input[name="pairing-code"]').value=${JSON.stringify(code)};document.querySelector('input[name="display-name"]').value='NA021 Profile B Edge';document.querySelector('form').requestSubmit();return true})()`);
  for(let i=0;i<60;i++){if(await c.evaluate('window.__claimHarness.ready'))break;await new Promise(ok=>setTimeout(ok,100));}
  console.log(await c.evaluate('window.__claimHarness.summary()'));
 }else if(phase==='release'){
  const {releaseAt}=JSON.parse(await fs.readFile(root+'release.json','utf8'));await c.evaluate(`window.__claimHarness.releaseAt(${releaseAt})`);console.log({scheduled:true});
 }else if(phase==='result'){
  const s=await c.evaluate('window.__claimHarness.summary()');await fs.writeFile(root+'evidence/profile-B.json',JSON.stringify(s,null,2));console.log(s);
 }else if(phase==='evaluate'){
  const expression=await fs.readFile(root+'edge-expression.js','utf8');const result=await c.evaluate(expression);await fs.writeFile(root+'evidence/edge-last.json',JSON.stringify(result,null,2));console.log(result);
 }else throw Error('unknown phase');
}finally{c.close();}
