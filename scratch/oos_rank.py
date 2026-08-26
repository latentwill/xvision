import json, os, sqlite3, statistics as st
from collections import defaultdict

DB = "/mnt/HC_Volume_105926998/docker/volumes/xvn-data/_data/xvn.db"
SDIR = "/mnt/HC_Volume_105926998/docker/volumes/xvn-data/_data/strategies"
db = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)

q = """
select r.agent_id, json_extract(s.body_json, '$.display_name'),
       json_extract(s.body_json, '$.granularity'), r.metrics_json, r.started_at
from eval_runs r
left join scenarios s on s.id = r.scenario_id
where r.status='completed' and r.metrics_json is not null and s.body_json is not null
  and json_extract(s.body_json, '$.display_name') not like 'Optimizer%'
"""
rows = db.execute(q).fetchall()
print("non-optimizer completed scored runs:", len(rows))

agg = defaultdict(list)
for aid, disp, gran, mj, ts in rows:
    try:
        m = json.loads(mj)
    except Exception:
        continue
    if "total_return_pct" not in m:
        continue
    b = m.get("baselines") or {}
    agg[aid].append({"scen": disp or "?", "gran": gran, "ret": m["total_return_pct"],
                     "sharpe": m.get("sharpe"), "dd": m.get("max_drawdown_pct"),
                     "wr": m.get("win_rate"), "ntr": m.get("n_trades"),
                     "bh": (b.get("buy_hold") or {}).get("return_pct"),
                     "tr": (b.get("simple_trend") or {}).get("return_pct")})


def name_of(aid):
    try:
        d = json.load(open(os.path.join(SDIR, aid + ".json")))
        return (d.get("manifest", d).get("name") or "?")
    except Exception:
        return "?"


out = []
for aid, rs in agg.items():
    if len(rs) < 4:
        continue
    rets = [r["ret"] for r in rs]
    grans = sorted(set(str(r["gran"]) for r in rs))
    sh = [r["sharpe"] for r in rs if r["sharpe"] is not None]
    # consistency: median across runs AND positive median within each granularity having >=2 runs
    per_g_ok = []
    for g in grans:
        gr = [r["ret"] for r in rs if str(r["gran"]) == g]
        if len(gr) >= 2:
            per_g_ok.append(st.median(gr))
    consistent = all(x > 0 for x in per_g_ok) if per_g_ok else False
    out.append({
        "aid": aid, "name": name_of(aid), "n": len(rets), "grans": ",".join(grans),
        "avg": st.mean(rets), "med": st.median(rets),
        "winrun": sum(1 for x in rets if x > 0) / len(rets),
        "sharpe": st.mean(sh) if sh else None,
        "consistent": consistent, "per_g_med": {g: st.median([r["ret"] for r in rs if str(r["gran"]) == g]) for g in grans},
        "rs": rs,
    })

out.sort(key=lambda s: s["med"], reverse=True)
hdr = ("strategy", "n", "grans", "avgRet", "medRet", "winRun%", "sharpe", "multiTF+")
print("%-44s %3s %-10s %8s %8s %8s %7s %9s" % hdr)
for s in out[:25]:
    print("%-44s %3d %-10s %8.2f %8.2f %8.0f %7.2f %9s" % (
        s["name"][:43], s["n"], s["grans"], s["avg"], s["med"],
        s["winrun"] * 100, s["sharpe"] or float("nan"), "YES" if s["consistent"] else "-"))

json.dump(out, open("/tmp/oos.json", "w"), default=str)
