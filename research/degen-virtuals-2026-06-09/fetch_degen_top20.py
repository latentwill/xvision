import requests, json, pathlib, time
from collections import Counter, defaultdict
from datetime import datetime, timezone

OUT=pathlib.Path('/home/agents/xvision/research/degen-virtuals-2026-06-09')
OUT.mkdir(parents=True, exist_ok=True)
s=requests.Session(); s.headers.update({'User-Agent':'Mozilla/5.0','accept':'application/json,*/*'})
leader=s.get('https://degen.virtuals.io/api/leaderboard',timeout=40).json()['data'][:20]

def hl(body):
    r=s.post('https://api.hyperliquid.xyz/info',json=body,timeout=40)
    try: return r.json()
    except Exception: return {'_status':r.status_code,'_text':r.text[:1000]}

def ffloat(x):
    try: return float(x)
    except Exception: return 0.0

def summarize_fills(fills):
    coins=Counter(); dirs=Counter(); pnls=defaultdict(float); vols=defaultdict(float)
    times=[]; winners=lossers=0; gross_win=gross_loss=0.0
    for f in fills if isinstance(fills,list) else []:
        c=f.get('coin','?'); coins[c]+=1; dirs[f.get('dir','?')]+=1
        px=ffloat(f.get('px')); sz=abs(ffloat(f.get('sz'))); vols[c]+=px*sz
        pnl=ffloat(f.get('closedPnl')); pnls[c]+=pnl
        if pnl>0: winners+=1; gross_win+=pnl
        elif pnl<0: lossers+=1; gross_loss+=pnl
        if f.get('time'): times.append(int(f['time']))
    return {
      'fill_count': len(fills) if isinstance(fills,list) else 0,
      'unique_coins': len(coins),
      'top_coins_by_fills': coins.most_common(10),
      'top_coins_by_volume': sorted(vols.items(), key=lambda kv: -kv[1])[:10],
      'pnl_by_coin': sorted(pnls.items(), key=lambda kv: -abs(kv[1]))[:12],
      'directions': dirs.most_common(),
      'open_long_fills': sum(v for k,v in dirs.items() if k.startswith('Open Long')),
      'open_short_fills': sum(v for k,v in dirs.items() if k.startswith('Open Short')),
      'close_long_fills': sum(v for k,v in dirs.items() if 'Close Long' in k),
      'close_short_fills': sum(v for k,v in dirs.items() if 'Close Short' in k),
      'closed_win_fills': winners, 'closed_loss_fills': lossers,
      'gross_win': gross_win, 'gross_loss': gross_loss,
      'first_fill_utc': datetime.fromtimestamp(min(times)/1000, timezone.utc).isoformat() if times else None,
      'last_fill_utc': datetime.fromtimestamp(max(times)/1000, timezone.utc).isoformat() if times else None,
    }

def summarize_state(st):
    positions=[]
    for ap in st.get('assetPositions',[]) if isinstance(st,dict) else []:
        p=ap.get('position',{})
        rec={k:p.get(k) for k in ['coin','szi','entryPx','positionValue','unrealizedPnl','returnOnEquity','liquidationPx','marginUsed','maxLeverage']}
        if isinstance(p.get('leverage'),dict): rec['leverage']=p['leverage']
        positions.append(rec)
    return {'marginSummary': st.get('marginSummary') if isinstance(st,dict) else None, 'positions': positions}

allrows=[]
for idx,a in enumerate(leader,1):
    aid=a['id']; addr=a.get('agentAddress') or (a.get('acpAgent') or {}).get('walletAddress')
    print('fetch', idx, aid, a['name'], addr)
    detail=s.get(f'https://degen.virtuals.io/api/agents/{aid}',timeout=40).json()
    forum=s.get(f'https://degen.virtuals.io/api/forums/{aid}',timeout=40).json()
    fills=hl({'type':'userFills','user':addr}) if addr else []
    state=hl({'type':'clearinghouseState','user':addr}) if addr else {}
    portfolio=hl({'type':'portfolio','user':addr}) if addr else []
    orders=hl({'type':'openOrders','user':addr}) if addr else []
    raw={'leaderboard_rank':idx,'leaderboard':a,'detail':detail,'forum':forum,'hyperliquid':{'userFills':fills,'clearinghouseState':state,'portfolio':portfolio,'openOrders':orders}}
    (OUT/f'agent_{idx:02d}_{aid}.json').write_text(json.dumps(raw,indent=2))
    perf=a.get('performance') or {}
    row={
      'rank': idx,'id': aid,'name': a.get('name'), 'symbol': a.get('tokenSymbol'), 'virtualId': a.get('virtualId'),
      'agentAddress': addr, 'tokenAddress': a.get('tokenAddress'), 'owner': (detail.get('data',{}).get('owner') if isinstance(detail,dict) else None),
      'official_agent_url': f'https://degen.virtuals.io/agents/{aid}',
      'virtuals_url': f"https://app.virtuals.io/virtuals/{a.get('virtualId')}" if a.get('virtualId') else None,
      'performance': perf,
      'copyTradeSelections': detail.get('copyTradeSelections') if isinstance(detail,dict) else None,
      'forum': forum.get('data') if isinstance(forum,dict) else None,
      'fill_summary': summarize_fills(fills),
      'state_summary': summarize_state(state),
      'open_orders': orders if isinstance(orders,list) else orders,
    }
    allrows.append(row)
    time.sleep(0.2)
(OUT/'top20_summary.json').write_text(json.dumps(allrows,indent=2))
print('wrote', OUT/'top20_summary.json')
