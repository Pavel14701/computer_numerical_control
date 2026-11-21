import json, urllib.request, hashlib, os

INBOX = "corpus/tier_b/inbox"

def get(url, raw=False):
    req = urllib.request.Request(url, headers={"User-Agent": "eid-tierb-collector",
        **({"Accept": "application/vnd.github.raw"} if raw else {})})
    with urllib.request.urlopen(req) as r:
        return r.read()

TARGETS = [
    ("mrRobot62/gcodebuilder", "app/gcode", "TEST_MillHolesGrid_10.5depth_T4.ngc"),
    ("mrRobot62/gcodebuilder", "app/gcode", "spiral_G3.ngc"),
    ("edelator/SCARA-Drawbot", "samples", "spirograph.gcode"),
    ("edelator/SCARA-Drawbot", "samples", "square.gcode"),
    ("edelator/SCARA-Drawbot", "samples", "hello.gcode"),
    ("edelator/SCARA-Drawbot", "samples", "pangram.gcode"),
]
for repo, sub, name in TARGETS:
    fname = {"TEST_MillHolesGrid_10.5depth_T4.ngc": "gcb_TEST_MillHolesGrid_10.5depth_T4.ngc",
             "spiral_G3.ngc": "gcb_spiral_G3.ngc",
             "spirograph.gcode": "edel_spirograph.gcode",
             "square.gcode": "edel_square.gcode",
             "hello.gcode": "edel_hello.gcode",
             "pangram.gcode": "edel_pangram.gcode"}[name]
    fp = os.path.join(INBOX, fname)
    if os.path.exists(fp):
        print("EXISTS", fname)
        continue
    try:
        data = json.loads(get(f"https://api.github.com/repos/{repo}/contents/{sub}/{name}"))
        sha = data["sha"]
        content = get(f"https://api.github.com/repos/{repo}/git/blobs/{sha}", raw=True)
        open(fp, "wb").write(content)
        print("OK", fname, len(content))
    except Exception as e:
        print("ERR", fname, e)
