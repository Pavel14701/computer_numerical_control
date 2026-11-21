import json, urllib.request, io, sys

sys.stdout = io.TextIOWrapper(sys.stdout.buffer, encoding="utf-8", errors="replace")

def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": "eid-tierb-collector"})
    with urllib.request.urlopen(req) as r:
        return r.read()

REPOS = [
    "rudra496/cnc",
    "Lachei/ImageGraver",
    "Lexvdpoel/Volura",
    "Vectric-Aspire-3D/Vectric-Aspire",
    "SmiCut/smiprog",
    "ShibaInuChan/Relief-Forge",
    "DominicZe-DZ/PB_Conversational",
]
EXTS = (".nc", ".ngc", ".gcode", ".tap")
out = io.StringIO()
W = out.write
for repo in REPOS:
    try:
        d = json.loads(get(f"https://api.github.com/repos/{repo}/git/trees/HEAD?recursive=1"))
        files = [t for t in d["tree"] if t["type"] == "blob" and t["path"].lower().endswith(EXTS)]
        W(f"== {repo} gcode-files: {len(files)}\n")
        for t in sorted(files, key=lambda x: x["path"])[:12]:
            W(f"   {t.get('size', 0):>8}  {t['path']}\n")
    except Exception as e:
        W(f"== {repo} ERR {e}\n")
open("corpus/tier_b/_diversity_cands2.txt", "w", encoding="utf-8").write(out.getvalue())
print("written")
