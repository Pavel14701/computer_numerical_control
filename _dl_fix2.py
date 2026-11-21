import json, urllib.request, hashlib, os

INBOX = "corpus/tier_b/inbox"

def get(url, raw=False):
    req = urllib.request.Request(url, headers={"User-Agent": "eid-tierb-collector",
        **({"Accept": "application/vnd.github.raw"} if raw else {})})
    with urllib.request.urlopen(req) as r:
        return r.read()

def fetch(repo, sub, name, fname):
    fp = os.path.join(INBOX, fname)
    if os.path.exists(fp):
        print("EXISTS", fname)
        return
    data = json.loads(get(f"https://api.github.com/repos/{repo}/contents/{sub}/{name}"))
    content = get(data["git_url"], raw=True)
    open(fp, "wb").write(content)
    print("OK", fname, len(content))

fetch("mrRobot62/gcodebuilder", "app/gcode", "TEST_MillHolesGrid_12.5deg_10.5depth_T4.ngc",
      "gcb_TEST_MillHolesGrid_12.5deg_10.5depth_T4.ngc")
fetch("mrRobot62/gcodebuilder", "app/gcode", "spiral_G3.ngc", "gcb_spiral_G3.ngc")

data = json.loads(get("https://api.github.com/repos/edelator/SCARA-Drawbot/contents/samples"))
print("SCARA samples dir:")
for e in data:
    print("  ", e["type"], e["name"], e.get("size", 0))
for e in data:
    if e["type"] != "file":
        continue
    fname = "edel_" + e["name"]
    fp = os.path.join(INBOX, fname)
    if os.path.exists(fp):
        print("EXISTS", fname)
        continue
    if (e.get("size") or 0) > 400_000:
        print("SKIP-большой", fname, e["size"])
        continue
    content = get(e["git_url"], raw=True)
    open(fp, "wb").write(content)
    print("OK", fname, len(content))
