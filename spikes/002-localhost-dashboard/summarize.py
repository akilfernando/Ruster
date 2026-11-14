"""Summarize measured observations, without FPS or parity extrapolation."""
import json,math,statistics
from pathlib import Path
ROOT=Path(__file__).resolve().parent/'evidence'
b=json.loads((ROOT/'native-baseline/results.json').read_text())
p=json.loads((ROOT/'browser-performance.json').read_text())[1:]
groups={'native_render_only':[x for x in b['rows'] if not x['warmup'] and x['render_only']],
        'native_readback_png':[x for x in b['rows'] if not x['warmup'] and not x['render_only']], 'browser':p}
keys=['native_total_ms','render_cpu_ms','submit_to_mapped_ms','png_encode_ms','host_roundtrip_ms','http_body_received_ms','image_decode_ms','request_to_raf_ms']
out={}
for name,rows in groups.items():
    out[name]={'n':len(rows)}
    for key in keys:
        if key in rows[0]:
            values=sorted(x[key] for x in rows)
            out[name][key]={'median':statistics.median(values),'max':max(values),'p95_nearest_rank':values[math.ceil(.95*len(values))-1]}
r=json.loads((ROOT/'wire-and-resources.json').read_text())
out['resources']=r
for state in ['idle','active']:
    out[state]={}
    for process in ['native','host']:
        a=r[state]['before'][process];c=r[state]['after'][process]
        out[state][process]={'one_core_cpu_percent':(c['cpu_ticks']-a['cpu_ticks'])/r['hz']/r[state]['seconds']*100,'rss_kib':c['rss_kib']}
page=json.loads((ROOT/'browser-idle.json').read_text());a={x['name']:x['value'] for x in page['before']['metrics']};c={x['name']:x['value'] for x in page['after']['metrics']}
out['browser_idle']={'task_duration_seconds':c['TaskDuration']-a['TaskDuration'],'js_heap_used_bytes':c['JSHeapUsedSize'],'measurement_seconds':page['seconds']}
out['browser_request_intervals_ms']=[b['request_start_ms']-a['request_start_ms'] for a,b in zip(p,p[1:])]
(ROOT/'performance-summary.json').write_text(json.dumps(out,indent=2))
print(json.dumps({k:v for k,v in out.items() if k!='resources'},indent=2))
