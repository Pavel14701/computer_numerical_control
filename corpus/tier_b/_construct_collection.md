# Конструкционно-целевой добор корпуса — отчёт сбора

Дата: 2026-11-21. Сборщик: агент сбора (конструкционно-целевой добор).
Куда: `corpus/tier_b/_staging/` (23 файла, НЕ inbox/ НЕ local — приёмка
после license-ревью оркестратором, CORPUS-CONTRACT §7).
Dedup: sha256 по содержимому против manifest.toml (121 tier_b) — дубликатов 0.
Не тронуты: manifest.toml, PROVENANCE.md, crates/**, inbox/, local/.

## 0. Префиксы источников (продолжение PROVENANCE.md)

| Префикс | Источник | Лицензия | Дата проверки | Tier-предложение |
|---|---|---|---|---|
| `lcnc_` | github.com/LinuxCNC/linuxcnc (master) | GPL-2.0 (API /license) | 2026-11-21 | local-dev |
| `cncsim_` | github.com/ismael-figueiredo/cnc-simulator (examples/) | GPL-3.0 (API /license) | 2026-11-21 | local-dev |
| `gce_` | github.com/0BAB1/gcodeEstimator (tests/g_test_files) | MIT (API /license) | 2026-11-21 | published |
| `mpe_` | github.com/cnc4me/macro-programming-examples (programs/, examples/) | MIT (API /license) | 2026-11-21 | published |
| `ncm_` | github.com/CNC-Master-xyz/NC-Code (Fanuc/, Haas/) | **NOASSERTION** — /license 404, LICENSE-файла в репо НЕТ | 2026-11-21 | **license unclear — local only** |

Примечание к `ncm_`: первичный grep поиска выдал «MIT» ошибочно (сдвиг пар
full_name/spdx_id в выдаче /search/repositories). Точечная проверка
/repos/.../license → 404, файла LICENSE в корне нет → трактуется как
NOASSERTION по правилам §7.

## 1. Инвентарь ДО добора (121 файл: 61 inbox + 60 local)

Реальные совпадения (word-boundary grep по содержимому, без ложных подстрок
типа G88⊂g881):

| # | Конструкция | Было в корпусе | Файлы | Статус |
|---|---|---|---|---|
| 1 | Cutter comp G41/G42+D/G40 | G41×8, G42×4 (реальных ~7) | gcb_ContourRect_G40/G41, gcb_ContourRoundRect_G42 (MIT); lcnc comp.ngc, comp-g1, comp311, comp311_2, gmoccapy_2_tools... | **покрыто → skip** (D-word в Fanuc-стиле — добор `ncm_g41_d_2d5_milling.nc`) |
| 2 | TLO G43/G44/G49 | G43×6 реальных, G49 — в шапках gcb | olle ×3 (MIT, `G43 Z10. H3`), lcnc gmoccapy change/change_g43, cds | G43 покрыто; Fanuc-идиома `G43 H# Z#.` — добор; **G44: 0 публичных сэмплов не найдено** |
| 3 | Синхр. шпинделя G33/G33.1/G84/G74 | **0** | — | дыра → собрано 4 файла |
| 4 | Сложные циклы G76/G87/G88 | G76-lathe ×3 (local); **G87/G88: 0** (g881/g882 — это O-sub remap, не циклы) | lcnc g76, ngcgui lathe g76base/g76diam | G76-mill/G87/G88 — дыры → собрано |
| 5 | Макросы G65/M98/M99/O | G65/G66/G67/M98/M99: **0**; O-коды ×4 local (LinuxCNC o<sub>-стиль) | lcnc change, change_g43, gridprobe, daisy | Fanuc-стиль — дыра → собрано 5 файлов |
| 6 | NURBS/сплайны | G5.2/G5.3 ×14 (LinuxCNC NURBS, local); **G5, G5.1, G6.1, G6.2: 0** | lcnc nurbs_G5_* ×14 | G5/G5.1/G6.x — дыра (см. сироты) |
| 7 | Трансформации G68/G69/G51/G50/G51.1/G68.2 | **0** | — | дыра → собрано (частично, см. сироты) |
| 8 | Полярные G15/G16/G12.1/G13.1 | **0** | — | дыра → собрано (частично, см. сироты) |
| 9 | Спец M-коды | M68×1 (probe_back_edge_angle, real); M70/M71/M73 — только в комментариях g20sub; M62–67: 0; M100–199: 0 реальных | lcnc probe_back_edge_angle | дыра → собрано 3 файла |
| 10 | Пробинг G31/G38 | G38.2/.3 ×3 (gridprobe + probe-макросы); **G31: 0** | lcnc gridprobe, change, probe_back_* | G38 покрыто → skip; G31 — сирота |
| 11 | Fanuc-ловушки | `;`-EOB: нет Fanuc-файлов; bare-int: нет; G10 L20: 0 в tier_b (tier_a #44 синтетика); G54.1: 0; G82 P-ms: 0 в tier_b (tier_a #18 — сек.) | — | дыры → собрано частично |

## 2. Собранные файлы (23, все в `corpus/tier_b/_staging/`)

Формат: путь → байт / sha256-12 → источник → license (дата) → диалект-предположение → ожидаемая диспозиц. (grbl / linuxcnc / fanuc-цель) → tier-предложение.

### 2.1 Синхронизация шпинделя (конструкция 3)

| Файл | Байт | sha256-12 | Источник | License | Диалект | Диспоз. (grbl/linuxcnc/fanuc) | Tier |
|---|---|---|---|---|---|---|---|
| `lcnc_g33_threading.ngc` | 933 | `7264674a9979` | raw .../LinuxCNC/linuxcnc/master/nc_files/threading.ngc | GPL-2.0 (2026-11-21) | linuxcnc | Err / **Ok** (g33 + o101-while) / n.a. | local-dev |
| `lcnc_g331_rigid_tap_docs.ngc` | 197 | `9fe5ea6bc197` | docs/src/gcode/g-code.adoc §G33.1 Example (verbatim, 5 строк) того же репо | GPL-2.0 (2026-11-21) | linuxcnc | Err / **Ok** (G33.1 Z K) / n.a. | local-dev |
| `gce_g74_peck_bareint.g` | 166 | `3b8e13f6215d` | raw .../0BAB1/gcodeEstimator/main/tests/g_test_files/TESTG74.g | MIT (2026-11-21) | fanuc-предп. | Err / Raw(G74)/ **eid-fanuc: G74 R Q** | published |
| `cncsim_g84_g43_placa.nc` | 1819 | `47a31c730b19` | raw .../cnc-simulator/main/examples/O2001_PLACA_D600.nc | GPL-3.0 (2026-11-21) | fanuc-предп. | Err / Raw(G84) / **eid-fanuc: G99 G84 Z R F** | local-dev |

### 2.2 Сложные циклы (конструкция 4)

| Файл | Байт | sha256-12 | Источник | License | Диалект | Диспоз. | Tier |
|---|---|---|---|---|---|---|---|
| `gce_g76_thread_fanuc.g` | 571 | `84ea8083fc92` | raw .../gcodeEstimator/.../TEST_G76.g | MIT | fanuc-lathe | Err / Raw(G76) / **eid-fanuc: G76 P Q R ×2 + `%` + O1000 + EOB `;`** | published |
| `cncsim_g76_eob_ciclos.nc` | 850 | `bb76ae343fac` | raw .../cnc-simulator/main/examples/O1002_CICLOS_GL300.nc | GPL-3.0 | fanuc-lathe (ROMI GL300) | Err / Raw / **eid-fanuc: G76 + `;`-EOB на каждой строке + `X20. ,C1.5` chamfer** | local-dev |
| `ncm_g87_g88_g121_turn1.nc` | 14126 | `67c08e3e18ee` | raw .../CNC-Master-xyz/NC-Code/main/Fanuc/TURN_1.NC | **NOASSERTION** | fanuc-lathe | Err / Err / **eid-fanuc: G87 back-bore ×6, G88 X R P ×6, G12.1/G13.1, G83, G76, G41-lathe** | **local only** |
| `gce_full_industrial_lathe.g` | 10361 | `1b02f7186cec` | raw .../gcodeEstimator/.../full.g | MIT | fanuc-lathe | Err / Err / **eid-fanuc: G10L2P1Z (беспробельно), G92S-clamp, G96/G95, G74 X Z P Q ×2, G28U0V0W0B0, M1, %** | published |

### 2.3 Макросы/подпрограммы (конструкция 5)

| Файл | Байт | sha256-12 | Источник | License | Диалект | Диспоз. | Tier |
|---|---|---|---|---|---|---|---|
| `mpe_g65_macro_deep_drill.nc` | 1036 | `30a54c7562f7` | raw .../cnc4me/macro-programming-examples/main/programs/DEEP_DRILL_CYCLE.nc | MIT | fanuc Macro B | Err / Err / **eid-fanuc: O7981 + G65 P7981 Z D S F (args) + IF[...] GOTO94x** | published |
| `mpe_g65_macro_material_verify.nc` | 3231 | `2b3f3e2fccb8` | raw .../programs/MATERIAL_VERIFICATION.nc | MIT | fanuc Macro B | Err / Err / **eid-fanuc: G65 P7999 X Y U V R A B C + IF/GOTO** | published |
| `mpe_g65_macro_centerline_probe.nc` | 2229 | `f8c4d145b129` | raw .../programs/CENTERLINE_VERIFICATION.nc | MIT | fanuc Macro B | Err / Err / **eid-fanuc: G65 P7998/P9832/P9810 (Renishaw-стиль probe-макросы) + IF/GOTO** | published |
| `mpe_while_clear_variables.nc` | 596 | `81ff1dc487c1` | raw .../examples/CLEAR_VARIABLE_RANGE.nc | MIT | fanuc Macro B | Err / Err / **eid-fanuc: WHILE[..]DO..END ×2** | published |
| `cncsim_m98_m99_duas_morsas.nc` | 798 | `359ce44bb049` | raw .../cnc-simulator/main/examples/O2002_DUAS_MORSAS_D600.nc | GPL-3.0 | fanuc-mill | Err / Err(E_UNSUPPORTED_SYNTAX — M98) / **eid-fanuc: M98 P2102 ×4, O2102/O2103 subs + M99 ×2** | local-dev |

### 2.4 TLO / cutter comp / трансформации (конструкции 1, 2, 7)

| Файл | Байт | sha256-12 | Источник | License | Диалект | Диспоз. | Tier |
|---|---|---|---|---|---|---|---|
| `cncsim_g154_g43_haas_vf2.nc` | 808 | `8e2f3e126640` | raw .../cnc-simulator/main/examples/O3001_HAAS_VF2.nc | GPL-3.0 | haas-mill | Err / Err / **eid-fanuc: `G43 Z50. H2 M8` (комбинир. строка) + G154 P1/P2 (Haas-аналог G54.1) + M99** | local-dev |
| `ncm_g43_g69_3d5x_milling.nc` | **2572288** | `14b1ddeffdf1` | raw .../CNC-Master-xyz/NC-Code/main/Fanuc/3D_5X_MILLING.NC | **NOASSERTION** | fanuc-mill 5x | Err / Err / **eid-fanuc: `G43 Z162. H1` + G69 + N-блоки + `%`; A0 C0 ротации** | **local only** |
| `ncm_g434_rtcp_5x_milling.nc` | **2100088** | `bcf53b66c8f7` | raw .../CNC-Master-xyz/NC-Code/main/Fanuc/5X_MILLING.NC | **NOASSERTION** | fanuc-mill 5x | Err / Err / **eid-fanuc: G43.4 (RTCP HPC) — единственный публичный сэмпл RTCP, найденный в GitHub** | **local only** |
| `ncm_g41_d_2d5_milling.nc` | 19053 | `184496ddcf6b` | raw .../CNC-Master-xyz/NC-Code/main/Fanuc/2.5D_MILLING.NC | **NOASSERTION** | fanuc-mill | Err / warning-or-Err / **eid-fanuc: `G41 X60.5 Y95.5 D3` (D-word!) ×десятки + N-нумерация** | **local only** |

Размерный cap: `ncm_g43_g69_3d5x` (2.5MB) и `ncm_g434_rtcp_5x` (2.1MB) —
исключения из cap 600KB: G69-отмена и G43.4-RTCP не встречаются больше ни в
одном публичном пермиссивном источнике (проверено поиском). Оркестратор может
отклонить при приёмке — тогда конструкции 7 (G69/G43.4) остаются без корпусного
доказательства (см. §4).

### 2.5 Полярные (конструкция 8)

| Файл | Байт | sha256-12 | Источник | License | Диалект | Диспоз. | Tier |
|---|---|---|---|---|---|---|---|
| `ncm_haas_g76_g15_eob_turn.nc` | 10923 | `58de645f410f` | raw .../CNC-Master-xyz/NC-Code/main/Haas/TURN.NC | **NOASSERTION** | haas-lathe | Err / Err / **eid-fanuc: G15/G112 (lathe-polar Haas), G76 K D A F (Haas-формат G76!), `;`-EOB, G41-lathe** | **local only** |
| `lcnc_polar_at_angle.ngc` | 398 | `619dd5622cf5` | raw .../LinuxCNC/linuxcnc/master/nc_files/polar.ngc | GPL-2.0 | linuxcnc | Err / **Ok** (`@r ^a` polar-слова LinuxCNC) / n.a. | local-dev |

Примечание: `lcnc_polar_at_angle.ngc` — смежная конструкция (LinuxCNC
`@радиус^угол`, не G15/G16) — добор диалектного разнообразия полярки.

### 2.6 Спец M-коды (конструкция 9)

| Файл | Байт | sha256-12 | Источник | License | Диалект | Диспоз. | Tier |
|---|---|---|---|---|---|---|---|
| `lcnc_m70_m72_demo.ngc` | 778 | `d57e5a29fafc` | raw .../LinuxCNC/.../nc_files/m70m72-demo.ngc | GPL-2.0 | linuxcnc | Err / **Ok** (M70 save + M72 restore + o-subs) / n.a. | local-dev |
| `lcnc_m73_demo.ngc` | 1139 | `b42bc793c958` | raw .../nc_files/m73-demo.ngc | GPL-2.0 | linuxcnc | Err / **Ok** (M73 autorestore) / n.a. | local-dev |
| `lcnc_m61_output_demo.ngc` | 793 | `07638c9ca187` | raw .../nc_files/m61demo.ngc | GPL-2.0 | linuxcnc | Err / Ok-or-Raw (M61 remap-демо, семейство output-M) / n.a. | local-dev |

M68 уже в корпусе (probe_back_edge_angle). M62–M65 в сыром виде не найдены
(см. §4). M100–199: реальных сэмплов не найдено (M101 M102.c в LinuxCNC
master недоступен — 404 по прежнему пути).

### 2.7 Fanuc-ловушки для eid-fanuc (конструкция 11)

| Ловушка | Файл(ы) в staging |
|---|---|
| `;`-EOB (конец блока `;`) | `cncsim_g76_eob_ciclos.nc` (каждая строка), `gce_g76_thread_fanuc.g` (`... ;`), `ncm_haas_g76_g15_eob_turn.nc` (Haas-стиль) |
| bare-integer DPI (X25 без точки) | `gce_g74_peck_bareint.g` (`G0X0Z0`, `G28B0`), `gce_g71_bareint_testg71.g` (`G0X20Z0`, `G01 Z-20`), `gce_full_industrial_lathe.g` (`G10L2P1Z139.5`) |
| G10 L-форма без пробелов | `gce_full_industrial_lathe.g` (G10L2P1Z) — L20 fanuc-форма не найдена (tier_a #44 — синтетика, в словаре GRBL) |
| G54.1 Pn | НЕ найдено публично; аналог — Haas `G154 P1/P2` в `cncsim_g154_g43_haas_vf2.nc` |
| P-dwell в цикле (мс, fanuc-семантика) | НЕ найдено публично (tier_a #18 — сек); Haas/TURN.NC содержит G81 R Z F без P; TURN_1.NC содержит G88 X R **P0** (P-слово в цикле!) |
| комбинированная `G43 H# Z#.` | `cncsim_g84_g43_placa.nc` ×7, `cncsim_g154_g43_haas_vf2.nc` (`G43 Z50. H2 M8`), `ncm_g43_g69_3d5x_milling.nc` (`G43 Z162. H1`); порядок слов Z-до-H; H-до-Z (`G43 H2 Z25.`) публично не найдено |
| `%`-обёртка + O-номер программы | `gce_g76_thread_fanuc.g`, `gce_full_industrial_lathe.g`, `ncm_*` (Fanuc), `cncsim_*` |
| bare T+M6 пары (T0101) | `gce_full_industrial_lathe.g` (`T0101G96S200M3`), `cncsim_g76_eob_ciclos.nc` (`T0101;`) |

## 3. Итог покрытия ПОСЛЕ добора

| # | Конструкция | Было | Стало (в staging) | Закрыто? |
|---|---|---|---|---|
| 1 | Cutter comp | 7 | +1 (Fanuc D-word) | **ДА** (было покрыто, добор качества) |
| 2 | TLO G43/G49 | 6–27 | +3 (комбинир. строки) | **ДА** (G44 — сирота) |
| 3 | G33/G33.1/G84/G74 | 0 | +4 | **ДА** (все 4 кода имеют сэмплы) |
| 4 | G76/G87/G88 | G76×3 | +4 | **ДА** (G87 ×6 вызовов, G88 ×6, G76 ×4 диалекта) |
| 5 | G65/M98/M99/O/MacroB | O-sub ×4 | +5 | **ДА** (G65+args, IF/GOTO, WHILE, M98/M99+O-subs) |
| 6 | G5/G5.1/G5.2/G6.x | G5.2/3×14 | +0 | **НЕТ** (G5-плоский, G5.1, G6.1, G6.2 — сироты; G5.2/3 покрыты) |
| 7 | G68/G69/G51/G50/G51.1/G68.2 | 0 | +3 (G69 ×2, G43.4 ×1) | **ЧАСТИЧНО**: G69, G43.4-RTCP есть; G68-set, G68.2, G51/G50-scale, G51.1/G50.1 — сироты |
| 8 | G15/G16/G12.1/G13.1 | 0 | +2 (G12.1/G13.1, G15-lathe) | **ЧАСТИЧНО**: G12.1/G13.1 есть; G16-mill, G13.1-pair есть... G16-mill — сирота (лиц. блок); LinuxCNC `@^` — бонус |
| 9 | M62–68/M70–73/M100–199 | M68×1 | +3 (M70/M72/M73/M61) | **ЧАСТИЧНО**: M68–73 семейство есть; M62–M65 raw, M100–199 — сироты |
| 10 | G31/G38 | G38×3 | +0 | **ДА по G38**; G31 — сирота |
| 11 | Fanuc-ловушки | 0 | 8 файлов-носителей | **ЧАСТИЧНО** (см. таблицу 2.7) |

**Итог: 7/11 конструкций закрыты корпусом полностью, 3 — частично, 1 (сетки
G5/G6-сплайны вне LinuxCNC-NURBS) — полностью без источников.**

## 4. Сироты — конструкции БЕЗ публичного корпусного доказательства
(проверено WebSearch-ом по GitHub/руководствам, 2026-11-21; ценный результат —
дыры без источников):

1. **G44** (TLO минус) — ни одного публичного сэмпла не найдено.
2. **G5, G5.1** (сплайны вне LinuxCNC), **G6.1, G6.2** (Fanuc NURBS) — вне
   LinuxCNC-NURBS (G5.2/G5.3 ×14 уже в local) публичных сэмплов нет.
3. **G68 2D-rotate set** (`G68 X Y R`) — найдены только tutorial-страницы
   (CNCCookbook/Haas-doc); в GitHub-сэмплах — только G69-cancel (взят).
4. **G68.2 / G53.1** (TWP) — нет .nc в GitHub; только постпроцессорные
   генераторы (Autodesk fanuc.cps) и forum-сниппеты.
5. **G51/G50 scale, G51.1/G50.1 mirror** — нет публичных сэмплов (G50 в
   lathe-сэмплах — spindle-clamp, иная семантика: `G92S2500`/`G50S1800`).
6. **G16 mill-polar** — публичный пример существует ТОЛЬКО на helmancnc.com
   с пометкой «© Helman CNC. All rights reserved» → сбор запрещён (лицензия
   не просто «unclear», а reserved). Haas-lathe G15/G112 взят как частичный
   аналог.
7. **G31 raw** — нет .nc сэмплов (G65 P9810 Renishaw-макросы в staging —
   единственное probe-доказательство fanuc-стороны).
8. **G10 L20 (fanuc-форма), G54.1 Pn, G82 P-ms** — нет публичных сэмплов
   (в corpus — только синтетика tier_a #44/#18; Haas-аналоги G154 P / P в
   G88 — собраны).
9. **M62–M65 raw usage** — LinuxCNC m61demo — соседний output-M-код (remap);
   сырых M62/M64-сэмплов не найдено (probe_back_edge_angle M68 — уже в
   корпусе).
10. **Несколько блоков на строке через `;`** (`X10 Y20; Z5;`) — все найденные
    fanuc/haas-файлы ставят `;` на КОНЦЕ строки; mid-line multi-block не
    найден.
11. **M100–199 user-M real-world** — нет сэмплов (LinuxCNC «M101 M102.c»
    недоступен в master; Okuma .min-макросы — другой диалект).

## 5. Пропущенные источники (license-блок / не собрано)

| Источник | Причина |
|---|---|
| helmancnc.com (G15/G16 bolt-circle) | «© Helman CNC. All rights reserved» — verbatim-копирование запрещено |
| franknaw/G-Code-Examples | license отсутствует (NOASSERTION); контент (bolt-hole/box-joint) уже перекрыт корпусом |
| sharpec/NC-Gcode-Fanuc (MIT) | Atom-пакет подсветки синтаксиса — NC-файлов нет |
| cnc4me/fanuc-macro-b (MIT) | тесты — inline-строки в .ts, отдельных .nc нет (сами программы взяты из соседнего репо macro-programming-examples) |
| Autodesk/cam-posteditor | постпроцессоры-генераторы, не NC-файлы |
| cnc-simulator examples/*.cncsim, learn/*.json | JSON-состояния симулятора, не NC |

## 6. Диспозиции — легенда

- **Ok** — ожидается успешный разбор текущим парсером (lcnc_* linuxcnc-цель).
- **Err/Raw(code)** — вне словаря grbl-1.1/linuxcnc → RAW-узел или диагностика
  (E_UNSUPPORTED_SYNTAX/SEMANTICS) на текущих парсерах.
- **eid-fanuc: ...** — целевая конструкция для парсера Fanuc (этапы 2–3);
  файлы fanuc-предположения собраны именно под этот тест-материал.

Партии: batch 1 = конструкции 1–6 (§2.1–2.3 + TLO/comp), batch 2 = 7–11
(§2.4–2.7). Все 23 файла: sha256 полные посчитаны, dedup против 121 = 0,
коммитов нет, manifest/PROVENANCE не тронуты.
