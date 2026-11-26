# kinematics_research.md — разведка формул кинематики непокрытых 5-осевых топологий

Итерация 15, подготовка фазы 12.5 (агент D, 2026-11-26).
Зона: research-отчёт. Крейты НЕ тронуты. Ничего не закоммичено.
Контекст: EID-L2-DESIGN.md амендменты v0.1.3 (§5.1: enum топологий ≥6, НЕ DH),
v0.1.4/v0.1.5 (оракул trtfuncs.c для XYZAC/XYZBC-trt, 12.5 разморожен).
Существующий порт: `crates/eid-l2/src/kinematics.rs` (xyzac/xyzbc-trt, TrtParams, Pose6).

---

## §1. Таблица вердиктов: топология → формулы → оракул → путь верификации

Нотация: R(w→m) — матрица поворота work→machine (правые матрицы, столбцы-векторы);
углы в градусах на границе API, внутри радианы (как в оракулах).

| # | Топология | Формулы | Источник (используемый) | Исполняемый код-оракул | Верификационный путь | Вердикт |
|---|-----------|---------|--------------------------|------------------------|----------------------|---------|
| 1 | **Table-table, вариант TT-1** (C-ротор СНИЗУ о станок, A/B-качающийся стол СВЕРХУ; R(w→m) = Rz(c)·Rx(a) / Rz(c)·Ry(b)) | ЕСТЬ (уже портированы) | LinuxCNC `trtfuncs.c` (du Preez) + docs 5-axis-kinematics.html | **ДА**: `xyzac-trt-kins`/`xyzbc-trt-kins` + vismach `table-rotary-tilting` + demo .ngc | Круговой инвариант + semantic_diff против trt-формул (правило v0.1.5) | **ПОКРЫТО** (сделано в `eid-l2/src/kinematics.rs`) |
| 2 | **Table-table, вариант TT-2 / «cradle», trunnion** (качающийся стол B СНИЗУ, ротор A/C СВЕРХУ на нём; R(w→m) = Ry(b)·Rx(a); Hermle-класс) | ЕСТЬ, полный fwd+inv | LinuxCNC `src/hal/components/xyzab_tdr_kins.comp` (D. Mueller) + CAD-Journal-2019 (та же структура Ry(B)·Rx(A) независимо) | **ДА**: sim-конфиг `configs/sim/axis/vismach/5axis/table-dual-rotary/` (vismach + demo ngc + remap M428/M429) | (а) round-trip против demo-конфига; (б) независимый кросс-чек CAD2019 (A=arctan(j/k), B=asin(i)); (в) hand-computed golden | **РЕАЛИЗУЕМО СЕЙЧАС** |
| 3 | **Head-table intersecting** (B-качание на шпинделе над C-столом, пивоты на одной вертикали; DMU 50-класс) | ЕСТЬ, полный fwd+inv (+якобиан, frames) | LinuxCNC `src/emc/kinematics/maxkins.c` (C. Radek) | **ДА**: sim `configs/sim/axis/vismach/5axis/max5/` (maxkins.ini + max5gui.py vismach) | (а) round-trip; (б) hand-computed golden (полярная форма); (в) degenerate-сверка с trt-вращением | **РЕАЛИЗУЕМО СЕЙЧАС** |
| 4 | **Head-table non-intersecting** (пивот B смещён от оси C — реальная геометрия Haas UMC-750, Fanuc-стандарт RT) | ЕСТЬ (общие замкнутые + структурно через nutating-модуль) | Greenwich/Chu 2025 (RRLLL-группа, закрытые формы, open PDF); структурный источник — `xyzacb_trsrn.comp` (пивоты Ly/Lz + смещения Dx/Dy) | 1:1-модуля НЕТ. Частичный: `trsrn` в degenerate-конфиге (nut=90°, primary=0); `max5` как intersecting-предел | (а) hand-computed golden из Greenwich Eqs (1)–(14); (б) кросс-чек спецслучаев (maxkins intersecting, trsrn degenerate); (в) round-trip | **РЕАЛИЗУЕМО С ОГОВОРКОЙ** (см. §3) — рекомендую «двойной независимый кросс-чек», машина-мишень не обязательна |
| 5 | **Head-head, универсальная ортогональная голова** (primary C' о вертикаль + secondary B' о горизонталь, пересекающиеся пивоты; Fidia/DMU 340P-класс) | ЕСТЬ | `trsrn` при nut-angle=90°: Rs вырождается в Ry(θ2) (проверено численно); сферическая форма — `5axiskins.c` | **ДА**: `tests/kins-twp/` (twpcheck.c + **независимый python-оракул** `remap_funcs_twp.py` + check.py) — in-tree тест, сверяющий C-модуль с python-матрицей по сетке углов | (а) in-tree kins-twp тест — образец дисциплины; (б) round-trip; (в) hand-computed golden | **РЕАЛИЗУЕМО СЕЙЧАС** |
| 6 | **Head-head nutating** (неперпендикулярные оси, ось secondary наклонена на nut-angle; She 2007 / Zeeko-класс) | ЕСТЬ, полный (TCP+TOOL, fwd+inv, пивоты/смещения) | `src/hal/components/xyzacb_trsrn.comp` + `xyzbca_trsrn.comp` + python-оракул `tests/kins-twp/*/remap_funcs_twp.py` + forum-тред 49103 (HTML-док в tar.gz вложении, в git НЕ входит) | **ДА**: тот же `tests/kins-twp` + vismach-конфиг `table-rotary_spindle-rotary-nutating/` + demo .ngc + G68.2/G53.x TWP-цепочка | (а) in-tree kins-twp тест; (б) round-trip; (в) She 2007/2008 — paywall, только как первоисточник ссылки | **РЕАЛИЗУЕМО СЕЙЧАС** (формулы + оракул полные) |
| 7 | **Gantry / bridge mill с вращающейся головой** (XYZBC(W): B-наклон + C-азимут на голове, W вдоль инструмента) | ЕСТЬ, полный fwd+inv+якобиан+tool-frame | LinuxCNC `src/emc/kinematics/5axiskins.c` (C. Radek) | **ДА**: sim `configs/sim/axis/vismach/5axis/bridgemill/` с demo `g532-fused-orient-move.ngc` (G68.2 + G53.2 STAY) и `g536-orient-then-move.ngc` (G53.6) | (а) rs274-native demo G-код с TWP; (б) round-trip; (в) hand-computed golden (сферич. форма) | **РЕАЛИЗУЕМО СЕЙЧАС** |
| 8 | **Table-head (TR)** | Косвенно | Порядок вложения R — перестановка уже собранных композиций (см. §4-примечание) | нет отдельной машины в источниках | — | **ОТЛОЖИТЬ**: реализовать по мере появления машины-мишени (не впрок) |

Сопутствующие источники (перекрёстные, без статуса оракула):
- Beckhoff TF5240 «Kinematic Transformation» (infosys.beckhoff.com): KIN_TYP_16/52/57/59/60/70 — параметрические таблицы HD-офсетов и геометрия нутационной головы; синусные формулы — в картинках (не извлекаются текстом); годится как семантическая сверка параметров (знаки HD9/HD10 = «conventional-directions»).
- Autodesk/Fusion-посты (напр. открытый `haas umc-750.cps`, github konkers/Mill-Post-Processors): содержат ТОЛЬКО решатель углов (axis-orientation → ABC) и диапазоны/предпочтения решений (UMC-750: B ∈ [−35°,110°]); позиционная RTCP-математика НЕ в посте (G43.4/TCPC на станке). Пост-дифф — валидный путь только для угловой части.
- She & Chang 2007 (IJMTM 47:537–545), She & Huang 2008 — paywall (Elsevier); это первоисточники нутационной головы, формулы в открытом доступе продублированы лучше (trsrn + Greenwich).
- URL из ТЗ `research.gre.ac.uk/3005/` устарел (сейчас там архитектурный сборник 2007 г.); актуальная работа группы Greenwich — GALA eprint **50013** (Chu et al. 2025, открытый PDF) и 32273 (2021, restricted).
- `genserkins.c`/`genserfuncs.c` — generic 6-joint serial **DH** с HAL-пинами (a/alpha/d + unrot) и численной (итерационной) инверсией: может служить третьим независимым кросс-чеком для любых serial-голов, но архитектурно мы от DH отказались (§5.1) — использовать только как оракул, не как образец.

---

## §2. Точные формулы по реализуемым топологиям

Все формулы выписаны «как в коде» (знаки и порядок операций сохранены) и
**проверены численно** round-trip-тестом (fwd∘inv = identity, ошибка ≤ 1e-13 на
1000 случайных конфигурациях с нулевыми и ненулевыми офсетами; скрипт
воспроизводит код 1-в-1). Запрос на проверку оркестратором: подтвердить
семантику букв/знаков под наш Pose6 (см. метки «⚠ сверить»).

### §2.1 Table-table TT-2 «cradle/trunnion» (XYZAB-tdr)

Источник: `src/hal/components/xyzab_tdr_kins.comp` (строки 59–150),
сим-конфиг `configs/sim/axis/vismach/5axis/table-dual-rotary/`.

Геометрия (README конфига): `rot-point` = пивот A/B-сборки; `dx/dz` — офсеты от
rot-point до центра лица стола A; при dx/dz ≠ 0 rot-point остаётся на оси B.
B — нижняя ось (о machine Y), A — верхний ротор (о локальной X), деталь на A.
R(w→m) = Ry(b)·Rx(a) (разложение прямых коэффициентов — совпадает покомпонентно).

Forward (joints JX,JY,JZ + углы a,b → pose; dt = tool-offset-z):
```
sa=sin(a); ca=cos(a); sb=sin(b); cb=cos(b)
px = JX − xr;  py = JY − yr;  pz = JZ − zr − dt
X =  cb·px + sb·pz + xr
Y =  sa·sb·px + ca·py − cb·sa·pz + sa·dz + yr
Z = −ca·sb·px + sa·py + ca·cb·pz − ca·dz + zr + dz + dt
```
Inverse (pose X,Y,Z + a,b → joints):
```
qx = X − xr − dx;  qy = Y − yr;  qz = Z − zr − dz − dt
JX =  cb·qx + sa·sb·qy − ca·sb·qz + cb·dx − sb·dz + xr
JY =  ca·qy + sa·qz + yr
JZ =  sb·qx − sa·cb·qy + ca·cb·qz + sb·dx + cb·dz + zr + dt
```
Квирк (сохранить в порте как есть, не «чинить»): forward **не зависит от dx**
(dx сокращается в паре fwd∘inv — проверено); dx — офсет именно inverse-стороны.
⚠ сверить: в нашем TrtParams аналогичная пара называлась x_offset/z_offset —
семантика dx тут иная (лицо стола A, а не пивот оси B как в trt XYZBC).

Независимый академический кросс-чек: CAD&Applications 16(6) 2019, Lin & Bohez,
с.1192, Eqs (5)–(11) — та же композиция Ry(B)·Rx(A): `A = Arctan(j_i/k_i) + kπ`,
`B = ArcSin(i_i) + 2kπ` (их знаковая конвенция синусов противоположна — их
примечание «sign of sine term…» §2.2.2); пост-умножение их Eq (2)
`[Ztrans T] = [Xtrans [Ytrans [Brot [Arot CL]]]]` структурно совпадает с
нашей парой fwd/inv (проверено покомпонентно в разведке).

### §2.2 Head-table intersecting (maxkins, XYZBC + U/V/W)

Источник: `src/emc/kinematics/maxkins.c`; sim `5axis/max5/`.
Голова (B, о machine Y, направление нестандартное — pin `conventional-directions`,
con = ±1) висит на Z-салазках и НЕ вращается с C-столом; C о machine Z через
machine origin (полярная форма). R(w→m) = Rz(c)∘[полярные поправки головы].

Forward (joints; R = pivot_length + tool_length + w):
```
zb = R·cos(b);  xb = R·sin(b)                      # поправка головы B
zv = u·sin(b);  xv = u·cos(b)                      # поправка U-салазок
mx = JX − con·xb − xv;   my = JY − v               # до поворота C
ρ = hypot(mx, my);  θ = atan2(my, mx) + rad(c)
X = ρ·cos(θ);  Y = ρ·sin(θ)
Z = JZ − zb − con·zv + pivot_length + tool_length
```
Inverse:
```
R = pivot_length + tool_length + w
zb = R·cos(b);  xb = R·sin(b)
ρ = hypot(X, Y);  θ = atan2(Y, X) − rad(c)
zv = u·sin(b);  xv = u·cos(b)
JX = ρ·cos(θ) + con·xb + xv;  JY = ρ·sin(θ) + v
JZ = Z + zb + con·zv − pivot_length − tool_length
```
Frames: work = Rz(−c); tool-ось (к конусу) = 3-й столбец Ry(con·b) =
(−con·sin b? ⚠ сверить знак) — в коде: `W joint runs the tip out along
(−con·sin B, 0, −cos B)`, third column of **Ry(con·B)**. Head-table вариант
Haas/Fanuc-стандарта получается при v=0, u=0, w=0 (тогда u/v/w-ветки выкидываются).

### §2.3 Head-head универсальная / nutating (trsrn; nut-angle ν ∈ [0°,90°])

Источники: `src/hal/components/xyzacb_trsrn.comp` (580 строк; парный
`xyzbca_trsrn.comp` — другой порядок букв), python-оракул
`tests/kins-twp/xyzacb/remap_funcs_twp.py`, in-tree тест `tests/kins-twp/`.
Машина: XYZ + A (стол-ротор) + B (secondary, нутационная) + C (primary головы).
**При ν = 90° вырождается в обычную ортогональную голову** (Rs → Ry(θ2),
проверено численно), при ν < 90° — нутация (She-класс); пивоты Ly/Lz и смещения
Dx/Dy покрывают **non-intersecting** геометрию.

Вращение TOOL-кинематики (work→machine): **R = Rp·Rs·Rtc**
```
Rp = Rz(θ1)                                   # primary
Rs = [[    Cs, −Cv·Ss,  Sv·Ss],
      [ Cv·Ss,      r,       t],              # нутационный secondary (Rodriges
      [−Sv·Ss,      t,       s]]              # об оси (0, Sv, Cv)); ν — nut-angle
r = Cs + Sv²·(1−Cs);  s = Cs + Cv²·(1−Cs);  t = Sv·Cv·(1−Cs)
Rtc = Rz(tc)                                  # virtual rotation о tool-Z
```
Обратные углы по вектору оси инструмента Kz=(Kzx,Kzy,Kzz) (python-оракул,
строки 144–199):
```
θ2 = acos( clamp( (Kzz − Cv²) / (1 − Cv²) ) )       # ветви ±θ2
θ1 = asin( clamp( (p·Kzy − t·Kzx) / (t² + p²) ) ),  p = Sv·sin(θ2)   # ветви θ1, π−θ1
достижимость: Kzz ≥ 2·Cv² − 1;  Kzz > 1−ε  ⇒  θ1=0, θ2=0 (вертикаль — бесконечно решений)
```
Трансляция TCP-inverse (comp, строки 239–274; Ly,Lz — пивоты головы, Dx,Dy —
смещения, Dray = y-rot-axis − (Dy+Ly), Draz = z-rot-axis − Lz, Dt = tool offset):
```
j[0] =  (Cp·SvSs − Sp·t)·(Dt+Lz) + Cp·Dx − (Cp·CvSs + Sp·r)·Ly − Dy·Sp − Dx + Qx
j[1] =  Cp·Dy + Dx·Sp − Cw·(Dray + Dy + Ly − Qy) + (Sp·SvSs + Cp·t)·(Dt+Lz)
        − (CvSs·Sp − Cp·r)·Ly − (Draz + Dt + Lz − Qz)·Sw + Dray
j[2] =  (Dt+Lz)·s + Ly·t − Cw·(Draz + Dt + Lz − Qz) + (Dray + Dy + Ly − Qy)·Sw + Draz
где Sw=sin(rad A) (стол), Ss=sin(rad B), Sp=sin(rad C) (primary), SvSs=Sv·Ss, CvSs=Cv·Ss
```
⚠ сверить: буквы в comp-заголовке («rotary table C») противоречат README/INI
(«A - table rotary, B - spindle secondary, C - spindle primary»); авторитет —
INI + python (A=стол). В наш порт переносить по INI-семантике.

Ключевой факт дисциплины оракула (README `tests/kins-twp`): «python-математика
написана НЕЗАВИСИМО от C-модуля… там где расходятся — у одного из двух знак или
порядок поворота неверен». Это готовый прецедент нашей методики
«формулы ≠ код, сверка на сетке углов».

### §2.4 Gantry/bridge mill (5axiskins, XYZBCW)

Источник: `src/emc/kinematics/5axiskins.c`; sim `5axis/bridgemill/`.
Сферическая форма: C — азимут (о Z), B — полярный угол от −Z (180°−B в s2r),
W — виртуальная ось вдоль инструмента (складывается в XYZ-джоинты).

```
s2r(r, t, p): x = r·sin(p)·cos(t); y = r·sin(p)·sin(t); z = r·cos(p)   # t,p в градусах
R = pivot_length + tool_length + w
r = s2r(R, c, 180 − b)  ⇒  (R·sin b·cos c, R·sin b·sin c, −R·cos b)
forward:  X = JX + r.x;  Y = JY + r.y;  Z = JZ + pivot+tool + r.z
inverse:  JX = X − r.x;  JY = Y − r.y;  JZ = Z − pivot − tool − r.z
```
Tool-frame (machine, столбец «к конусу» = 3-й): **Rz(C)·Ry(−B)**:
```
[ cb·cc, −sc, −sb·cc ]
[ cb·sc,  cc, −sb·sc ]
[ sb,     0,   cb    ]
```
Work-frame = identity (голова несёт инструмент, стол деталь не вращает).
Demo-G-код конфига гонит полную цепочку G68.2/G53.2/G53.6 — живой rs274-оракул
верхних слоёв (TWP/RTCP), не только математики.

### §2.5 Head-table non-intersecting (Haas UMC-750-класс) — сборка формулы

Готового 1:1 модуля нет; формула собирается из трёх независимых кусков:
1. **Вращение**: R(w→m) = Rz(c)·Ry(con·b) — та же пара, что maxkins (§2.2) без
   u/v; независимый источник — Greenwich/Chu 2025 (GALA eprint 50013, открытый
   PDF), группа RRLLL: forward Eqs (4)–(8), inverse Eq (12)
   `q_r = [Ω 0;0 1]·[Ω' 0;0 1]⁻¹·(p_r − Ω'ΘΛt − w)`, где w/t — векторы к пивотам
   стола/головы (Table 1 задаёт Ω,Θ,Λ для 10 групп R/L-цепочек). Модель
   верифицирована авторами на Mikron UCP 710, **DMU 50e/70e/340P**, Spinner U5-620.
2. **Трансляция непересекающихся пивотов**: структура trsrn (§2.3) — Ly/Lz +
   Dx/Dy + rot-axis pins; при ν=90°, θ1=0 (primary удержан) получаем ровно
   head-table (B-наклон + C-стол): R = Ry(b)·Rz(tc≈0) — проверено численно.
3. **Реальные константы UMC-750**: Haas публикует только процедуры выверки
   пересечения B/C и rotary-офсеты контроллера (#5181+ «centers of rotation …
   relative to linear axes home»); числовые пивоты — из docs машины, в формулы
   входят параметрами (как dy/dz в trt). ⚠ Не зашивать числа Haas в код —
   параметрами профиля.

---

## §3. «НУЖЕН ОРАКУЛ»: конкретные проверенные пути

Все перечисленные пути существуют в дереве github.com/LinuxCNC/linuxcnc
(проверено через git tree API 2026-11-26):

**Исполняемые sim-конфиги (vismach + switchkins + demo G-код):**
- `configs/sim/axis/vismach/5axis/bridgemill/` — 5axis.ini (5axiskins,
  MACHINE_MOVES_IN_MACHINE_FRAME=1), demo: `g532-fused-orient-move.ngc`,
  `g536-orient-then-move.ngc`, `5axisgui.ngc`; kinstype 0/1 переключение.
- `configs/sim/axis/vismach/5axis/table-dual-rotary/` — xyzab-tdr.ini
  (xyzab_tdr_kins), demo `demos/xyzab-tdr-demo.ngc`, remap M428/M429.
- `configs/sim/axis/vismach/5axis/table-rotary_spindle-rotary-nutating/` —
  `xyzacb-trsrn_twp/xyzacb-trsrn.ini`, `xyzbca-trsrn_twp/…`, demo .ngc
  (square.ngc, circular_pocket.ngc, incremental_repetition*.ngc), remap_subs,
  python/twp-helper-comp.py.
- `configs/sim/axis/vismach/5axis/table-rotary-tilting/` — trt (уже портирован):
  demos/boat-xyzac.ngc, impeller-7bl-xyzac.ngc, xyzac_switchkins_test_1..3.ngc.
- `configs/sim/axis/vismach/5axis/max5/` — max5kins.ini (maxkins) + max5gui.py.

**In-tree тест-оракул (образец дисциплины):**
- `tests/kins-twp/` — twpcheck.c (C-сторона) + `xyzacb/remap_funcs_twp.py` и
  `xyzbca/remap_funcs_twp.py` (НЕЗАВИСИМая python-сторона, numpy) + check.py
  (сетка углов frames + inverse-сверка кандидатов) + twp-xyzacb.ini /
  twp-xyzbca.ini. Это готовая схема «независимая реализация ↔ порт».
- `tests/kins-unwrap/` (walkcheck.c), `tests/kins-jacobian/`, `tests/kins-params/`
  (paritycheck.c) — дополнительные схемы.

**haltcl/HAL-исполнение на лету:** kinematics-модули — это hal-компоненты
(`halcompile --install kinsname.c`), исполняются симом LinuxCNC без железа;
`src/hal/components/userkins.comp` + `switchkinscomp.comp` позволяют подложить
произвольную кинематику. Живой round-trip-probe: запустить сим-конфиг, MDI-циклы
по сетке углов, читать joint→pos и обратно (kinstype 0 identity / 1 TCP).
HTML-документация с выводом формул trsrn — в аттаче forum-треда 49103
(table-rotary_spindle-rotary-nutating.tar.gz), **в git-дерево не входит** —
если понадобится вывод, брать оттуда.

**Для head-table non-intersecting (строка 4 §1)** — рекомендуемая процедура
вместо отсутствующего 1:1-оракула:
1. порт по Greenwich RRLLL Eq (12) + trsrn-структуре пивотов;
2. круговой инвариант на сетке (обязателен);
3. кросс-чек №1: maxkins intersecting-предел (офсеты→0) — совпадение поз;
4. кросс-чек №2: trsrn degenerate (ν=90°, primary=0) — совпадение вращений;
5. опционально кросс-чек №3: genserkins с DH-цепочкой, воспроизводящей
   голову (численная инверсия, точность ~1e-6);
6. фиксация golden-векторов в репозитории (как для trt).

Это удовлетворяет правилу G73-класса: каждый знак/порядок закрыт минимум двумя
независимыми источниками (python-оракул ≠ C-код ≠ статья).

---

## §4. Вопрос table-table ≡ XYZAC-trt: ОТВЕТ

**Частично да.** Заголовок trtfuncs.c прямо определяет trt как table-table:
«This mill has a tilting table (A axis) and horizontal rotary mounted to the
table (C axis)» — вращение R(w→m) = Rz(con·c)·Rx(con·a): **C — нижняя ось о
machine Z, A — верхняя (качающийся стол), C несут на станине**. Это вариант
TT-1 семейства table-table, и он **уже покрыт** портом `kinematics.rs`.

«Классический table-table» (Hermle C42 / trunnion, TT-2) — **другой порядок
вложения**: качающийся стол B снизу (о machine Y), ротор A/C сверху на нём:
R(w→m) = Ry(b)·Rx(a) / Ry(b)·Rz(c). trt-кодом НЕ покрывается (перестановка
сомножителей ≠ та же матрица). Но TT-2 имеет собственный полный оракул —
`xyzab_tdr_kins.comp` (§2.1) плюс независимую академическую сверку CAD2019.
Итог: **семейство table-table закрыто целиком** (TT-1 сделан, TT-2 реализуем
сейчас), вопрос «тopology vs классификация» снят: это одна семантика с двумя
порядками вложения, каждый со своим модулем формул.

Примечание о TR vs RT (table-head vs head-table): различие — порядок цепочки
(какая ротация ближе к детали), т.е. та же перестановка сомножителей. Оба
порядка присутствуют в собранном наборе (maxkins: Rz(c) снаружи; trsrn/tdr:
внутренние/внешние комбинации). Отдельной «топологии формул» TR не требуется —
это вариант bookkeeping'а внутри RT-семейства (см. строку 8 §1).

---

## §5. Рекомендация оркестратору: порядок реализации

**Top-2 «сразу» (формулы + исполняемый оракул + demo-G-код, максимальная
плотность независимых проверок):**
1. **TT-2 cradle/trunnion по xyzab_tdr_kins.comp** — закрывает Hermle-класс и
   «trunnion» из enum §5.1; оракул: sim-конфиг + CAD2019 кросс-чек + round-trip.
   Тот же стиль параметров, что у готового trt-порта (rot-point/offsets/tool) —
   минимальный дифф архитектуры (новый вариант enum + один модуль формул).
2. **Head-head/nutating по trsrn (+ 5axiskins для сферической формы)** —
   закрывает три строки enum сразу (head-head универсальный = ν90°, head-head
   nutating, gantry/bridge = 5axiskins); оракул — самый сильный в наборе
   (in-tree независимый python-тест kins-twp + demo .ngc с G68.2/G53.x).

**Следом:** 3. head-table intersecting по maxkins (просто, полярная форма) —
удобно делать вместе с non-intersecting, т.к. общий каркас пивотов.
4. head-table non-intersecting (UMC-750/Fanuc-стандарт — приоритет по §5.1
   «первая топология», но по правилу G73 закрывать после top-2: у него нет
   1:1-оракула, верификация — процедура из §3.5; положить golden-векторы в
   репозиторий до мержа).

**Отложить:** table-head (TR) — до появления машины-мишени; Beckhoff/genserkins
— только как кросс-чеки, не как образцы.

**Общая методика для всех портов (прецедент kins-twp):** независимый
Rust-порт формул (семантика, не копирование GPL-текста) + круговой инвариант
fwd∘inv на сетке углов с офсетами + минимум два независимых внешних источника
(C-код, python-оракул/статья/сим-конфиг) + golden-векторы в тестах.

---

### Приложение: числовая верификация разведки (2026-11-26)

Скрипт-воспроизведение формул §2 (python, 1-в-1 с кодом оракулов):
- xyzab_tdr: fwd∘inv worst err 1.14e-13 (1000 случайных, dx/dz≠0); forward
  независим от dx — подтверждено (квирк кода, не ошибка).
- maxkins: worst err 4.3e-14 (con=−1, pivot+tool+w, u/v≠0).
- 5axiskins: worst err 5.7e-14 (b∈(5°,175°), c полный круг).
- trsrn angle-решения: 799/799 ориентаций восстановлены точно (ν ∈ {30,45,60,90});
  Rs(ν=90°) ≡ Ry(θ2) покомпонентно.
