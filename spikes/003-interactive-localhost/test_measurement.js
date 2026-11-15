const assert=require('node:assert/strict');
const {ViewportCapture}=require('./measurement.js');
let c=new ViewportCapture();c.start('idle',0,'A');for(let t=0;t<20000;t+=16)c.paint('A',{decoded_at:t},t);let r=c.stop(20000,'expired');assert.equal(r.changed_updates_per_second,0);assert.equal(r.frames.length,0);
c.start('movement',0,'A');for(let i=1;i<=200;i++){c.decoded();c.paint(i%2?'B':'A',{decoded_at:i*50-10,request_to_decoded_ms:8,first_input_to_decoded_ms:12,latest_input_to_decoded_ms:7,session_token:'never export',png:'never export'},i*50)}r=c.stop(20000,'expired');assert.equal(r.frames.length,200);assert.equal(r.changed_updates_per_second,10);assert.equal(r.distributions.first_input_to_decoded_ms.median,12);assert(!JSON.stringify(r).includes('never export'));assert(!JSON.stringify(r).includes('session_token'));
c.start('hidden',0,'A');c.paint('B',{decoded_at:1},2,true);assert.equal(c.report.stop_reason,'hidden');
c.start('limit',0,'A');c.limit=2;for(let i=1;i<=3;i++)c.paint(String(i),{decoded_at:i},i);assert.equal(c.report.stop_reason,'buffer-limit');assert.equal(c.report.frames.length,2);
c.start('resources',0,'A');c.resource({processes:Object.fromEntries(['native','bridge'].map(n=>[n,{cpu_percent_one_core:25,rss_bytes:2097152,pss_bytes:1048576}]))});r=c.stop(1000,'disconnect');assert.equal(r.resource_summary.native.rss_mib.median,2);assert.equal(r.resource_summary.bridge.pss_mib.median,1);assert.equal(r.stop_reason,'disconnect');
c.start('expiry',0,'A');c.paint('B',{decoded_at:20000},20001);assert.equal(c.report.stop_reason,'expired');assert.equal(c.report.frames.length,0);
console.log('PASS idle zero, changed PNG transitions, >64 retention, timing, privacy, hidden/disconnect/expiry stop, buffer cap, resource MiB');
