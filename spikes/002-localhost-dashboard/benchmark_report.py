"""Derive tables and a plot only from retained measured records."""
import json,math,statistics,hashlib
from pathlib import Path
from benchmark import OUT,ROOT,save
s=json.loads((OUT/'summary.json').read_text());resources={};lines=['# Browser phase timings (ms; median / nearest-rank p95; N=20 each)','', '| Workload | CPU prepare | Submit → mapped | PNG encode | Native total | Host roundtrip | HTTP body received | Decode | Request → two RAF |','| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |']
keys=['render_cpu_ms','submit_to_mapped_ms','png_encode_ms','native_total_ms','host_roundtrip_ms','http_body_received_ms','image_decode_ms','request_to_raf_ms']
for name,cohorts in s.items():
    lines.append('| '+name+' | '+' | '.join(f"{cohorts['browser'][k]['median']:.2f} / {cohorts['browser'][k]['p95']:.2f}" for k in keys)+' |')
    d=json.loads((OUT/(name+'-direct/results.json')).read_text());b=json.loads((OUT/(name+'-browser/results.json')).read_text());resources[name]={}
    for tag,obs in [('direct_active',d['active']),('browser_active',b['active']),('browser_idle',b['idle'])]:
        if not obs:continue
        resources[name][tag]={'seconds':obs['seconds'],'processes':{}}
        for process,a in obs['after'].items():
            before=obs['before'][process]
            resources[name][tag]['processes'][process]={'cpu_percent_one_core':(a['cpu_s']-before['cpu_s'])/obs['seconds']*100,'rss_after_mib':a['rss']/1048576,'rss_delta_mib':(a['rss']-before['rss'])/1048576}
    assert len(b['rows'])==20 and len(b['edit_rows'])==20
    assert all(b['rows'][i]['frames']<b['rows'][i+1]['frames'] for i in range(19))
    assert [r['revision'] for r in b['edit_rows']]==list(range(1,21))
    assert b['burst']['after']['revision']-b['burst']['before']['revision']==1
    cleanup=json.loads((OUT/(name+'-browser/cleanup.json')).read_text());assert all(cleanup['pids_gone'].values()) and cleanup['capability_removed']
(OUT/'phases.md').write_text('\n'.join(lines)+'\n');save(OUT/'resources.json',resources)
svg=['<svg xmlns="http://www.w3.org/2000/svg" width="960" height="430" viewBox="0 0 960 430"><rect width="960" height="430" fill="#111820"/><g fill="#e0edf2" font-family="sans-serif"><text x="25" y="30" font-size="20">Matched pipeline cost — median and p95 (ms), N=20</text><text x="25" y="55" font-size="13">768×512, 4× MSAA · offscreen native baseline, not native-window parity</text>']
colors=['#88aaff','#dbb56b','#8fe1c6'];modes=[('native-only','native_total_ms','render only'),('native-png','native_total_ms','native PNG'),('browser','request_to_raf_ms','browser → 2 RAF')]
for i,(name,c) in enumerate(s.items()):
    y=95+i*100;svg.append(f'<text x="25" y="{y+25}" font-size="15">{name}</text>')
    for j,(mode,key,label) in enumerate(modes):
        v=c[mode][key];yy=y+j*24
        svg.append(f'<rect x="220" y="{yy}" width="{v["median"]*8:.1f}" height="16" fill="{colors[j]}"/><path d="M {220+v["p95"]*8:.1f} {yy-2} v20" stroke="{colors[j]}" stroke-width="2"/><text x="{235+v["p95"]*8:.1f}" y="{yy+13}" font-size="12">{label}: {v["median"]:.2f} / {v["p95"]:.2f}</text>')
svg.append('<text x="25" y="410" font-size="12">Bars: medians · ticks: nearest-rank p95 · RAF is a paint-opportunity proxy, not physical visibility.</text></g></svg>');(OUT/'latency.svg').write_text(''.join(svg))
identity=json.loads((OUT/'identity.json').read_text());identity['measured_source_sha256']={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in ['src/main.rs','src/authority.rs','server.py','dashboard.html']};save(OUT/'identity.json',identity)
print('Verified 3 workloads × (20 native + 20 PNG + 20 browser + 20 edit/Undo) =',sum(20*4 for _ in s),'measured phase samples; warmup excluded. Burst drops observed, not passed.')
