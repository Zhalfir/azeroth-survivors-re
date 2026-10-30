#!/usr/bin/env python3
"""Summarise survivors combat logs: python3 analyze.py <round-dir> [...]"""
import json, sys, glob, os, statistics as st
from collections import defaultdict

def runs(paths):
    for d in paths:
        for f in sorted(glob.glob(os.path.join(d, "*.jsonl"))):
            recs = []
            for line in open(f, errors="replace"):
                try: recs.append(json.loads(line))
                except Exception: pass
            if any(r.get("type") == "end" for r in recs):
                yield os.path.basename(f), recs

def summarise(name, recs):
    start = next(r for r in recs if r["type"] == "start")
    end = next(r for r in recs if r["type"] == "end")
    ticks = [r for r in recs if r["type"] == "tick"]
    spells, taken = defaultdict(float), defaultdict(float)
    level_at = {}
    for t in ticks:
        for s, v in t["spells"].items(): spells[s] += v["damage"]
        for s, v in t["taken"].items(): taken[s] += v["damage"]
        for lv in (5, 10, 20, 30):
            if t["level"] >= lv and lv not in level_at: level_at[lv] = t["t"]
    fps = sorted(t["fps"] for t in ticks if t["t"] > 3 and t["fps"] > 0)
    # Old logs carry no cpu_ms/gpu_ms (gpu_ms only with WOW_GPU_MS=1): None, shown as "-".
    cpu = sorted(t["cpu_ms"] for t in ticks if t["t"] > 3 and "cpu_ms" in t)
    gpu = sorted(t["gpu_ms"] for t in ticks if t["t"] > 3 and "gpu_ms" in t)
    total = sum(spells.values()) or 1
    top = sorted(spells.items(), key=lambda kv: -kv[1])[:4]
    hurt = sorted(taken.items(), key=lambda kv: -kv[1])[:3]
    bosses = [r["msg"] for r in recs if r["type"] == "event" and ("arrives" in r["msg"] or "slain" in r["msg"])]
    return dict(
        cls=start["class"], map=start["map"], diff=start["difficulty"], won=end["won"], t=end["t"],
        level=end["level"], kills=end["kills"], dmg=end["damage"], blow=end.get("killing_blow"),
        level_at=level_at, top=[(s, round(100*v/total)) for s, v in top], hurt=[(s, round(v)) for s, v in hurt],
        fps_p50=fps[len(fps)//2] if fps else 0, fps_p10=fps[len(fps)//10] if fps else 0,
        cpu_p50=cpu[len(cpu)//2] if cpu else None, gpu_p50=gpu[len(gpu)//2] if gpu else None,
        alive_max=max((t["alive"] for t in ticks), default=0), bosses=len([b for b in bosses if "slain" in b]))

def ms(vals):
    """Median of the runs' values in ms, or "-" when no run has one."""
    vals = [v for v in vals if v is not None]
    return f"{st.median(vals):.1f}" if vals else "-"

def main():
    rows = [summarise(n, r) for n, r in runs(sys.argv[1:])]
    by = defaultdict(list)
    for r in rows: by[r["cls"]].append(r)
    print(f"{'class':8} {'runs':>4} {'won':>3} {'surv med':>8} {'surv max':>8} {'lvl':>4} {'kills':>6} {'lv5':>5} {'lv10':>5} {'fps50':>5} {'fps10':>5} {'cpu50':>5} {'gpu50':>5}  top spells / killers")
    for c, rs in sorted(by.items()):
        surv = [r["t"] for r in rs]
        med = lambda k: st.median([r["level_at"].get(k, 9999) for r in rs])
        print(f"{c:8} {len(rs):4} {sum(r['won'] for r in rs):3} {st.median(surv):8.0f} {max(surv):8.0f} {st.median([r['level'] for r in rs]):4.0f} {st.median([r['kills'] for r in rs]):6.0f} {med(5):5.0f} {med(10):5.0f} {st.median([r['fps_p50'] for r in rs]):5.0f} {st.median([r['fps_p10'] for r in rs]):5.0f} {ms([r['cpu_p50'] for r in rs]):>5} {ms([r['gpu_p50'] for r in rs]):>5}  {rs[0]['top']} | {[r['blow']['from'] if r['blow'] else 'won' for r in rs]}")
    if "-v" in os.environ.get("ANALYZE", ""):
        for r in rows: print(r)

main()
