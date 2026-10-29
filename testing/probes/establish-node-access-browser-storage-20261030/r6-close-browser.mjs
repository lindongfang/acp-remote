import {connectPrivate} from './private-cdp.mjs';
const c=await connectPrivate();
try{await c.send('Browser.close');console.log('Dedicated r6 private browser closed')}finally{c.close()}
