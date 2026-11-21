import json, urllib.request, urllib.parse, io, sys

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": "eid-tierb-collector"})
    with urllib.request.urlopen(req) as r:
        return r.read()

GOOD = {"MIT", "Apache-2.0", "BSD-3-Clause", "BSD-2-Clause", "CC0-1.0"}
queries = [
    "thread milling gcode",
    "helical milling gcode",
    "gcode pocket example",
    "cnc drilling gcode",
    "linuxcnc sample ngc",
    "gcode relief cnc",
    "3d milling gcode",
]
cands = {}
for q in queries:
    try:
        data = json.loads(get(f"https://api.github.com/search/repositories?q={urllib.parse.quote(q)}&sort=stars&per_page=10"))
    except Exception as e:
        print("Q ERR", q, e)
        continue
    for it in data.get("items", []):
        lic = (it.get("license") or {}).get("spdx_id")
        if lic not in GOOD:
            continue
        if it["full_name"] not in cands:
            cands[it["full_name"]] = (lic, it["stargazers_count"], (it.get("description") or "")[:60])

out = io.StringIO()
W = out.write
for name, (lic, st, desc) in sorted(cands.items(), key=lambda kv: -kv[1][1]):
    W(f"{name} | {lic} | {st} | {desc}\n")
open("corpus/tier_b/_diversity_search.txt", "w", encoding="utf-8").write(out.getvalue())
print("candidates:", len(cands))
