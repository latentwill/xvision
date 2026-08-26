import json, os, sqlite3, statistics as st

DB = "/mnt/HC_Volume_105926998/docker/volumes/xvn-data/_data/xvn.db"
SDIR = "/mnt/HC_Volume_105926998/docker/volumes/xvn-data/_data/strategies"
db = sqlite3.connect(f"file:{DB}?mode=ro", uri=True)

FINALISTS = [
    "01KV3ARXGJDW5R5JNK39Y5PQRV",  # gemini-flash-trend-rider-tight-stop
    "01KVB3T3Q1P10RD4NQVJVWHQ25",  # mean-reversion-5m-vibethinker
    "01KVNCAK68N2FBJ2TPR2F55ACD",  # btc-15m-meanrev-glm52
    "01KTYBP2STJMZVJ9CAZY6S6PQ9",  # orb-breakout-15m-ollama-fino1
]

for aid in FINALISTS:
    p = os.path.join(SDIR, aid + ".json")
    print("=" * 100)
    try:
        d = json.load(open(p))
        man = d.get("manifest", d)
        print("ID:", aid)
        print("name:", man.get("name") or man.get("display_name"))
        for k in ("decision_mode", "decision_cadence_minutes", "description"):
            if man.get(k):
                print("%s: %s" % (k, str(man[k])[:200]))
        flt = d.get("filter")
        if flt:
            print("filter:", json.dumps(flt)[:400])
        risk = d.get("risk_config") or man.get("risk_config")
        if risk:
            print("risk:", json.dumps(risk)[:300])
        agents = d.get("agents") or []
        for ref in agents[:2]:
            row = db.execute("select name, description from agents where agent_id=?",
                             (ref.get("agent_id"),)).fetchone()
            if row:
                print("agent:", row[0], "|", str(row[1])[:160])
                slots = db.execute(
                    "select name, provider, model, system_prompt from agent_slots where agent_id=? order by slot_index",
                    (ref.get("agent_id"),)).fetchall()
                for s in slots:
                    print("   slot:", s[0], "|", s[1], "|", s[2])
                    sp = s[3] or ""
                    print("   prompt[:600]:", sp[:600].replace("\n", " "))
    except Exception as e:
        print(aid, "ERR", e)

    q = """select json_extract(s.body_json,'$.display_name'), json_extract(s.body_json,'$.granularity'),
                  r.metrics_json
           from eval_runs r left join scenarios s on s.id=r.scenario_id
           where r.status='completed' and r.agent_id=? and r.metrics_json is not null
             and json_extract(s.body_json,'$.display_name') not like 'Optimizer%'"""
    print("-- scored non-optimizer runs --")
    for disp, gran, mj in db.execute(q, (aid,)).fetchall():
        m = json.loads(mj)
        b = m.get("baselines") or {}
        bh = (b.get("buy_hold") or {}).get("return_pct")
        trd = (b.get("simple_trend") or {}).get("return_pct")
        print("  %-6s %-42s ret=%7.2f wr=%4.0f%% trades=%4d dd=%5.1f | BH=%7.2f simpleTrend=%7.2f" % (
            gran, (disp or "?")[:42], m["total_return_pct"], (m.get("win_rate") or 0) * 100,
            m.get("n_trades") or 0, m.get("max_drawdown_pct") or 0,
            bh if bh is not None else float("nan"),
            trd if trd is not None else float("nan")))
