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

# pocketRRect_#13_001.gcode (с '#' в имени) пропущен — contents API не
# резолвит имя с '#' без esc; остальные 5 ниже покрывают цель.
fetch("mrRobot62/gcodebuilder", "app/gcode", "ContourRoundRect_A70B100_G42_TD-8.75.gcode", "gcb_ContourRoundRect_A70B100_G42_TD-8.75.gcode")
fetch("mrRobot62/gcodebuilder", "app/gcode", "ContourRect_TD3.0_A20_B30_D-2.5_G41.ngc", "gcb_ContourRect_G41.ngc")
fetch("mrRobot62/gcodebuilder", "app/gcode", "drillHolesGrid_A10B10_3x3_tD6_45.gcode", "gcb_drillHolesGrid_A10B10_3x3_tD6_45.gcode")
fetch("mrRobot62/gcodebuilder", "app/gcode", "ContourHolesOnGrid_4x2y_A30_B20_90deg.ngc", "gcb_ContourHolesOnGrid_90deg.ngc")
fetch("mrRobot62/gcodebuilder", "app/gcode", "ContourRect_TD3.0_A20_B30_D-2.5_G40.ngc", "gcb_ContourRect_G40.ngc")
