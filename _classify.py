import re, collections, os, json

INBOX = "corpus/tier_b/inbox"

def classify(name: str, text: str) -> dict:
    fam = set()
    # Z-значения в ходах (rapids/feeds) → multi-level pocketing / Z-stuffing
    zvals = sorted({round(float(m), 3) for m in re.findall(r"^[^\n]*?\bZ(-?\d+\.?\d*)", text, re.M)})
    neg_z = [z for z in zvals if z < 0]
    if len(neg_z) >= 2:
        fam.add("F1: multi-level Z (2+ отрицательных Z-глубин)")
    # сверловка: G81/G82/G83/G73 или .tap drill-файл
    if re.search(r"\bG8[1-3]\b|\bG73\b", text) or name.endswith(".tap"):
        fam.add("F2: сверловка (G81/G83-класс / .tap)")
    # G18/G19
    if re.search(r"\bG1[89]\b", text):
        fam.add("F3: G18/G19 дуги")
    # helical/thread: Z меняется в строке с G2/G3, или thread-ключевое слово
    arc_z = 0
    for line in text.splitlines():
        if re.match(r"\s*[Gg]0?[23]\b", line) and re.search(r"\bZ-?\d", line):
            arc_z += 1
    if arc_z:
        fam.add("F4: helical/thread (винтовые G2/G3 с Z)")
    if re.search(r"thread", name + text[:2000], re.I):
        fam.add("F4: helical/thread (ключевое слово)")
    # 3D contours: много разных Z (5+) — Z-стуфинг/рельеф
    if len(zvals) >= 5:
        fam.add("F5: 3D-контуры/Z-стуфинг (5+ Z-уровней)")
    # радиусные цепочки: 2+ дуг G2/G3 с разными радиусами (I/J/R варьируются)
    radii = set()
    for line in text.splitlines():
        m = re.search(r"\bR(-?\d+\.?\d*)", line)
        if re.match(r"\s*[Gg]0?[23]\b", line):
            if m:
                radii.add(round(float(m.group(1)), 3))
            ij = re.findall(r"\b[IJ](-?\d+\.?\d*)", line)
            if ij:
                r2v = float(ij[0]) ** 2 + (float(ij[1]) ** 2 if len(ij) > 1 else 0.0)
                radii.add(round(r2v ** 0.5, 3))
    if len(radii) >= 2:
        fam.add("F6: радиусные цепочки (2+ разных радиуса)")
    if not fam:
        fam.add("F0: 2D (без Z-проходов/дуг-классов)")
    return fam

rows = []
for fn in sorted(os.listdir(INBOX)):
    text = open(os.path.join(INBOX, fn), encoding="utf-8", errors="replace").read()
    fams = classify(fn, text)
    rows.append((fn, sorted(fams)))

cnt = collections.Counter()
for fn, fams in rows:
    for f in fams:
        cnt[f.split(":")[0]] += 1
out = io.StringIO() if False else None
res = {"files": [(f, fl) for f, fl in rows], "families": dict(cnt)}
json.dump(res, open("corpus/tier_b/_family_class.json", "w", encoding="utf-8"), ensure_ascii=False, indent=1)
for fn, fams in rows:
    print(fn, "->", "; ".join(fams))
print("\nСемейства:", dict(cnt))
