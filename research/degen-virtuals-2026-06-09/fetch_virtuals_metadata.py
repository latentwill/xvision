import json, pathlib, requests, time
OUT=pathlib.Path('/home/agents/xvision/research/degen-virtuals-2026-06-09')
summary=json.loads((OUT/'top20_summary.json').read_text())
s=requests.Session(); s.headers.update({'User-Agent':'Mozilla/5.0','accept':'application/json,*/*','origin':'https://app.virtuals.io','referer':'https://app.virtuals.io/'})
for a in summary:
    vid=a.get('virtualId')
    if not vid:
        a['virtuals_metadata']=None
        continue
    url=f'https://api2.virtuals.io/api/virtuals/{vid}?populate=genesis,vibesInfo'
    print('fetch virtual', a['rank'], a['name'], vid)
    r=s.get(url,timeout=40)
    try:
        meta=r.json().get('data')
    except Exception:
        meta={'_status':r.status_code,'_text':r.text[:1000]}
    a['virtuals_metadata']=meta
    (OUT/f"virtuals_{vid}.json").write_text(json.dumps(meta,indent=2))
    time.sleep(0.15)
(OUT/'top20_summary_enriched.json').write_text(json.dumps(summary,indent=2))
print('wrote enriched')
