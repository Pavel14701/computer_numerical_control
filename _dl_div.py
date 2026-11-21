import json, urllib.request, urllib.parse, hashlib, os

INBOX = "corpus/tier_b/inbox"

def get(url):
    req = urllib.request.Request(url, headers={"User-Agent": "eid-tierb-collector"})
    with urllib.request.urlopen(req) as r:
        return r.read()

SEL = [
    ("vis_", "Jackhammer88/Visu", "Visu/GcodeExamples/SmilinJack - ThreeAxisFlatMachine Relief.tap"),
    ("vis_", "Jackhammer88/Visu", "Visu/GcodeExamples/knight.tap"),
    ("vis_", "Jackhammer88/Visu", "Visu/GcodeExamples/hse1p.tap"),
    ("gcb_", "mrRobot62/gcodebuilder", "app/gcode/TEST_MillPocketCircle_SO30.ngc"),
    ("gcb_", "mrRobot62/gcodebuilder", "app/gcode/TEST_MillPocketCircle_SO50.ngc"),
    ("gcb_", "mrRobot62/gcodebuilder", "app/gcode/pocketRRect_001_02.gcode"),
    ("gcb_", "mrRobot62/gcodebuilder", "app/gcode/pocketRRect_001a.gcode"),
    ("gcb_", "mrRobot62/gcodebuilder", "app/gcode/pocketCircle_oR30_iR0_T6_tD-2.5.gcode"),
    ("gcb_", "mrRobot62/gcodebuilder", "app/gcode/TEST_MillHolesGrid_10.5depth_T4.ngc"),
    ("gcb_", "mrRobot62/gcodebuilder", "app/gcode/contourMillHolesGrid_001.gcode.ngc"),
    ("gcb_", "mrRobot62/gcodebuilder", "app/gcode/drillHoles_R100_nH4_tD-2.5_0.00deg.gcode"),
    ("gcb_", "mrRobot62/gcodebuilder", "app/gcode/ContourHolesOnGrid_4x2y_A30_B20_22.5deg.ngc"),
    ("gcb_", "mrRobot62/gcodebuilder", "app/gcode/spiral_G3.ngc"),
    ("gcb_", "mrRobot62/gcodebuilder", "app/gcode/TEST_DrillHolesGrid_45deg_T4.ngc"),
    ("vince_", "vincedovy/CNC-files", "BewareOfDog v10.nc"),
    ("vince_", "vincedovy/CNC-files", "BewareOfDog v10_2_T2.nc"),
    ("vince_", "vincedovy/CNC-files", "BewareOfDog v10_3_T4.nc"),
    ("vince_", "vincedovy/CNC-files", "BrushRabbitv4.nc"),
    ("vince_", "vincedovy/CNC-files", "tictactoe1.nc"),
    ("olle_", "ollema/gcode", "bricka short wall.tap"),
    ("olle_", "ollema/gcode", "bricka slots.tap"),
    ("olle_", "ollema/gcode", "top cont.tap"),
    ("olle_", "ollema/gcode", "top trace finish.tap"),
    ("olle_", "ollema/gcode", "ivar engraving test.tap"),
    ("edel_", "edelator/SCARA-Drawbot", "samples/spirograph.gcode"),
    ("edel_", "edelator/SCARA-Drawbot", "samples/square.gcode"),
    ("edel_", "edelator/SCARA-Drawbot", "samples/hello.gcode"),
    ("edel_", "edelator/SCARA-Drawbot", "samples/pangram.gcode"),
]

seen = {}
for d in (INBOX, "corpus/tier_b/local"):
    for fn in os.listdir(d):
        seen[hashlib.sha256(open(os.path.join(d, fn), "rb").read()).hexdigest()] = fn

rows = []
for prefix, repo, path in SEL:
    fname = prefix + os.path.basename(path).replace(" ", "_")
    fp = os.path.join(INBOX, fname)
    if os.path.exists(fp):
        content = open(fp, "rb").read()
    else:
        content = None
        for ref in ("HEAD", "master", "main"):
            url = f"https://raw.githubusercontent.com/{repo}/{ref}/" + urllib.parse.quote(path)
            try:
                content = get(url)
                break
            except Exception:
                continue
        if content is None:
            print("FAIL-404", path)
            continue
    h = hashlib.sha256(content).hexdigest()
    if h in seen:
        print("DUP", path)
        continue
    seen[h] = path
    open(fp, "wb").write(content)
    rows.append((fname, f"{repo}/{path}", len(content), h[:12]))
    print("OK", fname, len(content))

json.dump(rows, open("corpus/tier_b/_tmp_rows3.json", "w"), indent=1)
import collections
hs = collections.Counter()
for fn in os.listdir(INBOX):
    hs[hashlib.sha256(open(os.path.join(INBOX, fn), "rb").read()).hexdigest()] += 1
print("INBOX published:", sum(hs.values()), "unique:", len(hs), "| новые:", len(rows))
