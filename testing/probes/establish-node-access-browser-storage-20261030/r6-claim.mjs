import {connectPrivate} from './private-cdp.mjs';
const fs=await import('node:fs/promises'),c=await connectPrivate();
try{
 const {code}=JSON.parse(await fs.readFile('D:/Project/acp-remote/.tmp/browser-storage-acceptance-20261030/r6-pair-input.json','utf8'));
 const expression=`(()=>{const code=document.querySelector('input[name="pairing-code"]'),name=document.querySelector('input[name="display-name"]');if(!code||!name)throw Error('Missing pair form');const set=Object.getOwnPropertyDescriptor(HTMLInputElement.prototype,'value').set;set.call(code,${JSON.stringify(code)});set.call(name,'NA031 r6 private test');for(const e of [code,name]){e.dispatchEvent(new Event('input',{bubbles:true}));e.dispatchEvent(new Event('change',{bubbles:true}))}document.querySelector('form').requestSubmit();return true})()`;
 console.log({pairFormSubmitted:await c.evaluate(expression)});
}finally{c.close()}
