import {connect} from './cdp.mjs';const c=await connect(process.argv[2]);try{await c.send('Browser.close');console.log('Closed dedicated browser '+process.argv[2])}finally{c.close()}
