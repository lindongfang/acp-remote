import {connectPrivate} from './private-cdp.mjs';
const c=await connectPrivate();
try{console.log(await c.evaluate(`({paragraphs:[...document.querySelectorAll('form p')].map(p=>p.innerText),form:document.querySelector('form')?.innerText})`))}finally{c.close()}
