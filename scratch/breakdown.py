import json, os, sqlite3, statistics as st
from collections import defaultdict

DB = "/mnt/HC_Volume_105926998/docker/volumes/xvn-data/_data/xvn.db"
SDIR = "/mnt/HC_Volume_105926998/docker/volumes/xvn-data/_data/strategies"
db = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)

cands = json.load(open("/tmp/lb.json"))
short = [aid for aid, s in cands if s["n"] >= 9][:10]

q = """
select r.agent_id, json_extract(s.body_json, '$.display_name'),
       json_extract(s.body_json, '$.granularity'), r.metrics_json, r.mode, r.started_at
from eval_runs r
left join scenarios s on s.id = r.scenario_id
where r.status='completed' and r.metrics_json is not null and r.agent_id=?
"""


def strat_meta(aid):
    p = os.path.join(SDIR, aid + ".json")
    try:
        d = json.load(open(p))
    except Exception:
        return "?", "?", []
    man = d.get("manifest", d)
    name = man.get("name") or man.get("display_name") or "?"
    cadence = man.get("decision_cadence_minutes")
    models = []
    for ref in d.get("agents", []) or []:
        row = db.execute("select name from agents where agent_id=?", (ref.get("agent_id"),)).fetchone()
        if row:
            models.append(row[0])
        else:
            models.append(ref.get("agent_id", "?")[:12])
    return name, cadence, models


for aid in short:
    rows = db.execute(q, (aid,)).fetchall()
    name, cadence, models = strat_meta(aid)
    print("=" * 100)
    print(f"{aid}\n{name} | cadence={cadence}min | models={models}")
    bygran = defaultdict(list)
    byscen = defaultdict(list)
    for _, disp, gran, mj, mode, ts in rows:
        try:
            m = json.loads(mj)
        except Exception:
            continue
        b = m.get("baselines") or {}
        rec = {"ret": m["total_return_pct"], "sharpe": m.get("sharpe"),
               "bh": (b.get("buy_hold") or {}).get("return_pct")}
        bygran[gran].append(rec)
        byscen[(gran, disp or "?")].append(rec)
    for gran in sorted(bygran, key=str):
        rs = bygran[gran]
        rets = [r["ret"] for r in rs]
        sh = [r["sharpe"] for r in rs if r["sharpe"] is not None]
        alb = [r["ret"] - r["bh"] for r in rs if r["bh"] is not None]
        print("  gran=%-5s n=%-4d avgRet=%7.2f medRet=%7.2f win%%=%4.0f sharpe=%6.2f alphaBH=%7.2f" % (
            gran, len(rets), st.mean(rets), st.median(rets),
            100 * sum(1 for x in rets if x > 0) / len(rets),
            st.mean(sh) if sh else float("nan"),
            st.mean(alb) if alb else float("nan")))
    for (gran, scen), rs in sorted(byscen.items()):
        rets = [r["ret"] for r in rs]
        print("      %-6s %-42s n=%-3d avgRet=%8.2f medRet=%8.2f" % (
            gran, scen[:42], len(rets), st.mean(rets), st.median(rets)))
