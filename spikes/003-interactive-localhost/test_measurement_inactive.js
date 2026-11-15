'use strict';
const assert=require('node:assert/strict');
const fs=require('node:fs');
const path=require('node:path');
const vm=require('node:vm');
let summaryWrites=0,timerClears=0;
const elements=new Map();
function element(id){
 if(!elements.has(id))elements.set(id,{disabled:false,set textContent(value){if(id==='measureSummary')summaryWrites++;}});
 return elements.get(id);
}
const context={
 document:{hidden:false,createElement:()=>({open:false,innerHTML:''}),querySelector:()=>({before(){}}),getElementById:element},
 window:{},navigator:{onLine:true},performance:{now:()=>100},
 addEventListener(){},clearTimeout(){timerClears++;},setTimeout(){throw Error('Inactive capture must not schedule work');},
 current:null,autoEnabled:false,
};
vm.runInNewContext(fs.readFileSync(path.join(__dirname,'measurement.js'),'utf8'),context);
const measurement=context.window.viewportMeasurement;
measurement.capture.start('movement',0,'before',false);
measurement.capture.paint('after',{decoded_at:5,request_to_decoded_ms:4},10,false);
measurement.capture.stop(20,'manual');
const before={summaryWrites,timerClears};
for(let i=0;i<1000;i++)measurement.paint('settled',{decoded_at:80});
assert.equal(summaryWrites,before.summaryWrites,'Stopped measurement must not rewrite the report on every subsequent frame');
assert.equal(timerClears,before.timerClears,'Stopped measurement must not repeatedly perform finish/cleanup per frame');
console.log('PASS: inactive measurement adds no repeated report rendering or timer cleanup');
