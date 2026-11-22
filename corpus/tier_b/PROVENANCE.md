# Tier B — provenance-журнал входящих файлов (CORPUS §7)

Дата сбора: 2026-11-20. Сборщик: Агент D (автоматический, GitHub API +
raw.githubusercontent.com). Метод дедупликации: sha256 по содержимому —
в итоговом inbox **72 файла, дубликатов содержимого: 0** (1 дубль cncjs/
LinuxCNC-семпла пропущен на приёме). Файлы лежат в `corpus/tier_b/inbox/`.
В manifest.toml НЕ внесены, не парсились — приёмка в корпус после
проверки лицензий оркестратором (CORPUS §7).

**Задокументированное ограничение партии (директива итерации 4, 2026-11-21):**
published-набор **2D/2.5D-skewed** (SVG-конверсии, гравировка, роутер-плоские);
3-осевой mill-контент (pocketing с Z-проходами, реальные G81/G83-цепочки из
CAM-постов, 3D-контуры/Z-уровни, helical ramp entry, thread milling) — добор
в **итерации 5** (post-acceptance), постоянный diversity-чек-лист сбора.
Прецедент: collection без content-diversity чек-листа даёт перекос.

**Ruling оркестратора (CORPUS v0.6.4, 2026-11-20) — два контура:**
* **published** — пермиссивные лицензии (MIT/Apache/CC0/...) → остаётся в
  `tier_b/inbox/`, публикуется с корпусом;
* **local-dev** — GPL/неясные → `tier_b/local/`, тестируются parse-or-
  diagnose, **никогда не публикуются**. Parse-or-diagnose критерий CORPUS §7
  работает и на local-файлах.

Именование: `lcnc_<dir-слаг>_<имя>` — источники LinuxCNC; `grbljs_<имя>` —
источники cncjs (GRBL-предположение).

Отброшено при сборе: 3 файла >600KB (cncjs github.gcode 4.4MB, part.gcode
2.0MB, octocat.gcode 1.1MB) — размерный cap; 1 content-dup.

---

## Источник 1: LinuxCNC/linuxcnc (master), 60 файлов

- URL базы: https://github.com/LinuxCNC/linuxcnc (raw: raw.githubusercontent.com/LinuxCNC/linuxcnc/master/<path>)
- Дата: 2026-11-20
- Лицензия проекта: **GPL-2.0** (GitHub API license: GNU GPL v2.0).
- Per-file авторство: в master НЕТ README каталога nc_files (найден только
  `nc_files/orphans/README`, целиком: «abandoned or superseded .ngc files»);
  авторы отдельных примеров в дереве master не документированы.
- Диалект-предположение: **linuxcnc** (родной репозиторий контроллера).
- СТАТУС ЛИЦЕНЗИИ: **сомнительный → РЕШЕНО: local-dev** (ruling
  оркестратора, CORPUS v0.6.4) — файлы перемещены в `tier_b/local/`,
  тестируются, никогда не публикуются.
- Каталоги выборки: nc_files/ (корень), ngcgui_lib (+lathe), macros/,
  probe/basic_probe/macros/, remap-subroutines/, nurbs/G5/ (полностью),
  tests/trajectory-planner/circular-arcs/nc_files/quick-tests/ + arc-feed/,
  tests/interp/good/, configs/sim/gmoccapy/macros/.

| Файл (tier: **local-dev**) | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `lcnc_configs_sim_gmoccapy_macros_change.ngc` | `configs/sim/gmoccapy/macros/change.ngc` | 1924 | `3d4d204d2164` |
| `lcnc_configs_sim_gmoccapy_macros_change_g43.ngc` | `configs/sim/gmoccapy/macros/change_g43.ngc` | 601 | `6059250a6807` |
| `lcnc_configs_sim_gmoccapy_macros_go_to_position.ngc` | `configs/sim/gmoccapy/macros/go_to_position.ngc` | 399 | `d6796e350d93` |
| `lcnc_nc_files_3D_Chips.ngc` | `nc_files/3D_Chips.ngc` | 200509 | `b0d584021e7a` |
| `lcnc_nc_files_3dtest.ngc` | `nc_files/3dtest.ngc` | 907 | `fdd71703dc76` |
| `lcnc_nc_files_arcspiral.ngc` | `nc_files/arcspiral.ngc` | 31066 | `f0d6cb86835c` |
| `lcnc_nc_files_b-index.ngc` | `nc_files/b-index.ngc` | 178 | `a4718bb4e606` |
| `lcnc_nc_files_cds.ngc` | `nc_files/cds.ngc` | 6637 | `a667b1283bd3` |
| `lcnc_nc_files_comp-g1.ngc` | `nc_files/comp-g1.ngc` | 896 | `f9fca8b2a5b0` |
| `lcnc_nc_files_comp.ngc` | `nc_files/comp.ngc` | 535 | `3d2d89ef729c` |
| `lcnc_nc_files_comp311.ngc` | `nc_files/comp311.ngc` | 527 | `97c5b16dcaa3` |
| `lcnc_nc_files_comp311_2.ngc` | `nc_files/comp311_2.ngc` | 521 | `9c4b35273167` |
| `lcnc_nc_files_cone.ngc` | `nc_files/cone.ngc` | 704 | `d210c191f1cb` |
| `lcnc_nc_files_daisy.ngc` | `nc_files/daisy.ngc` | 5249 | `b01a80491fef` |
| `lcnc_nc_files_factorial.ngc` | `nc_files/factorial.ngc` | 256 | `eb59f71655b6` |
| `lcnc_nc_files_flowsnake.ngc` | `nc_files/flowsnake.ngc` | 1104 | `a4d1dfb199ef` |
| `lcnc_nc_files_g20sub.ngc` | `nc_files/g20sub.ngc` | 204 | `8ad9c730fe5f` |
| `lcnc_nc_files_g76.ngc` | `nc_files/g76.ngc` | 1505 | `6dce99b51d2d` |
| `lcnc_nc_files_g881.ngc` | `nc_files/g881.ngc` | 660 | `c0d29ff37af5` |
| `lcnc_nc_files_g881min.ngc` | `nc_files/g881min.ngc` | 94 | `f40cc65a5438` |
| `lcnc_nc_files_g882.ngc` | `nc_files/g882.ngc` | 102 | `5e7a8d106c1c` |
| `lcnc_nc_files_gmoccapy_2_tools_with_cutter_radius_compensation.ngc` | `nc_files/gmoccapy_2_tools_with_cutter_radius_compensation.ngc` | 1234 | `3bf0812c7c1e` |
| `lcnc_nc_files_gridprobe.ngc` | `nc_files/gridprobe.ngc` | 961 | `789b1d45e30d` |
| `lcnc_nc_files_hole-circle.ngc` | `nc_files/hole-circle.ngc` | 280 | `58a916535397` |
| `lcnc_nc_files_increment.ngc` | `nc_files/increment.ngc` | 56 | `8c40c65933e0` |
| `lcnc_nc_files_ngcgui_lib_arc1.ngc` | `nc_files/ngcgui_lib/arc1.ngc` | 10349 | `1dbe6f30b34c` |
| `lcnc_nc_files_ngcgui_lib_arc2.ngc` | `nc_files/ngcgui_lib/arc2.ngc` | 1571 | `68d3dcacb1a9` |
| `lcnc_nc_files_ngcgui_lib_backlash.ngc` | `nc_files/ngcgui_lib/backlash.ngc` | 2404 | `8a809a17061c` |
| `lcnc_nc_files_ngcgui_lib_db25.ngc` | `nc_files/ngcgui_lib/db25.ngc` | 1564 | `b93d01afa086` |
| `lcnc_nc_files_ngcgui_lib_gosper.ngc` | `nc_files/ngcgui_lib/gosper.ngc` | 2128 | `86701f90bae9` |
| `lcnc_nc_files_ngcgui_lib_lathe_g76base.ngc` | `nc_files/ngcgui_lib/lathe/g76base.ngc` | 4233 | `6412b0cedd54` |
| `lcnc_nc_files_ngcgui_lib_lathe_g76diam.ngc` | `nc_files/ngcgui_lib/lathe/g76diam.ngc` | 2447 | `73316b9828cf` |
| `lcnc_nc_files_nurbs_G5_Curva_Chitarra__1_3_G5.ngc` | `nc_files/nurbs/G5/Curva_Chitarra #1=3_G5.ngc` | 910 | `5b7d43c8cede` |
| `lcnc_nc_files_nurbs_G5_Curva_Delfino__1_1_G5.ngc` | `nc_files/nurbs/G5/Curva_Delfino #1=1_G5.ngc` | 1782 | `3f63ae03627e` |
| `lcnc_nc_files_nurbs_G5_Curva_Mazatrol__1_1_G5.ngc` | `nc_files/nurbs/G5/Curva_Mazatrol #1=1_G5.ngc` | 701 | `db35c7359714` |
| `lcnc_nc_files_nurbs_G5_Curva_Stella__1_1_G5.ngc` | `nc_files/nurbs/G5/Curva_Stella #1=1_G5.ngc` | 997 | `8dee97d28185` |
| `lcnc_nc_files_nurbs_G5_butterfly-xy.ngc` | `nc_files/nurbs/G5/butterfly-xy.ngc` | 1732 | `a9b028fd729e` |
| `lcnc_nc_files_nurbs_G5_butterfly-yz.ngc` | `nc_files/nurbs/G5/butterfly-yz.ngc` | 1732 | `324da3186dd6` |
| `lcnc_nc_files_nurbs_G5_butterfly-zx.ngc` | `nc_files/nurbs/G5/butterfly-zx.ngc` | 1731 | `e9b16731414f` |
| `lcnc_nc_files_nurbs_G5_leto-xy.ngc` | `nc_files/nurbs/G5/leto-xy.ngc` | 514 | `2a6a6a56e5c8` |
| `lcnc_nc_files_nurbs_G5_nurbs-xy.ngc` | `nc_files/nurbs/G5/nurbs-xy.ngc` | 331 | `96627567ad93` |
| `lcnc_nc_files_nurbs_G5_nurbs-yz.ngc` | `nc_files/nurbs/G5/nurbs-yz.ngc` | 331 | `7de88bad9da4` |
| `lcnc_nc_files_nurbs_G5_nurbs-zx.ngc` | `nc_files/nurbs/G5/nurbs-zx.ngc` | 331 | `993238ac94d9` |
| `lcnc_nc_files_nurbs_G5_trident-xy-yz-zx.ngc` | `nc_files/nurbs/G5/trident-xy-yz-zx.ngc` | 866 | `6f9ded45b11d` |
| `lcnc_nc_files_nurbs_G5_trident-xy.ngc` | `nc_files/nurbs/G5/trident-xy.ngc` | 354 | `452d2b77ee4b` |
| `lcnc_nc_files_nurbs_G5_trident-yz.ngc` | `nc_files/nurbs/G5/trident-yz.ngc` | 354 | `12132cdeeb46` |
| `lcnc_nc_files_nurbs_G5_trident-zx.ngc` | `nc_files/nurbs/G5/trident-zx.ngc` | 354 | `dbe4f8804b21` |
| `lcnc_nc_files_probe_basic_probe_macros_probe_back_edge_angle.ngc` | `nc_files/probe/basic_probe/macros/probe_back_edge_angle.ngc` | 3644 | `3ef5b160179e` |
| `lcnc_nc_files_probe_basic_probe_macros_probe_back_left_inside.ngc` | `nc_files/probe/basic_probe/macros/probe_back_left_inside.ngc` | 3334 | `754e2c17a3e1` |
| `lcnc_nc_files_probe_basic_probe_macros_probe_back_left_outside.ngc` | `nc_files/probe/basic_probe/macros/probe_back_left_outside.ngc` | 3274 | `8224d131996a` |
| `lcnc_nc_files_remap-subroutines_change.ngc` | `nc_files/remap-subroutines/change.ngc` | 340 | `b9e23a37c276` |
| `lcnc_nc_files_remap-subroutines_on_abort.ngc` | `nc_files/remap-subroutines/on_abort.ngc` | 269 | `7b6810836a37` |
| `lcnc_tests_interp_good_good-arc.big.imperial.center-format.ngc` | `tests/interp/good/good-arc.big.imperial.center-format.ngc` | 64 | `6c10c0966bc5` |
| `lcnc_tests_interp_good_good-arc.big.metric.center-format.ngc` | `tests/interp/good/good-arc.big.metric.center-format.ngc` | 65 | `8790523ccf6f` |
| `lcnc_tests_trajectory-planner_circular-arcs_nc_files_arc-feed_arc_maxvel_xy.ngc` | `tests/trajectory-planner/circular-arcs/nc_files/arc-feed/arc_maxvel_xy.ngc` | 86 | `99f2166ab52f` |
| `lcnc_tests_trajectory-planner_circular-arcs_nc_files_arc-feed_arc_time.ngc` | `tests/trajectory-planner/circular-arcs/nc_files/arc-feed/arc_time.ngc` | 222 | `43c283c91926` |
| `lcnc_tests_trajectory-planner_circular-arcs_nc_files_quick-tests_arc-arc-concave-convex.ngc` | `tests/trajectory-planner/circular-arcs/nc_files/quick-tests/arc-arc-concave-convex.ngc` | 82 | `414a0fcc61d7` |
| `lcnc_tests_trajectory-planner_circular-arcs_nc_files_quick-tests_arc-arc-convex-concave.ngc` | `tests/trajectory-planner/circular-arcs/nc_files/quick-tests/arc-arc-convex-concave.ngc` | 82 | `9099745c1a0c` |
| `lcnc_tests_trajectory-planner_circular-arcs_nc_files_quick-tests_arc-arc-convex-convex.ngc` | `tests/trajectory-planner/circular-arcs/nc_files/quick-tests/arc-arc-convex-convex.ngc` | 71 | `59e8babc373b` |
| `lcnc_tests_trajectory-planner_circular-arcs_nc_files_quick-tests_arc-arc-neartangent.ngc` | `tests/trajectory-planner/circular-arcs/nc_files/quick-tests/arc-arc-neartangent.ngc` | 85 | `7e8b086bbab8` |

---

## Источник 2: cncjs/cncjs (examples/gcode), 12 файлов

- URL базы: https://github.com/cncjs/cncjs (raw: raw.githubusercontent.com/cncjs/cncjs/master/examples/gcode/<name>)
- Дата: 2026-11-20
- Лицензия: **MIT** (GitHub API license: MIT License) — чистая.
- Диалект-предположение: **grbl** (cncjs — хост-контроллер GRBL-семейства;
  примеры — классический GRBL-словарь: arc-ijk, full-circle-without-xyz,
  helical-thread-milling и т.п.).
- Отброшено: github.gcode / part.gcode / octocat.gcode (>600KB);
  arc-xy-plane.gcode — content-dup (пропущен).

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `grbljs_Circle_Diamond_Square_Calibration.gcode` | `cncjs/cncjs/examples/gcode/Circle_Diamond_Square_Calibration.gcode` | 16940 | `5865813fdc22` |
| `grbljs_arc-ijk.gcode` | `cncjs/cncjs/examples/gcode/arc-ijk.gcode` | 37 | `422d02b22305` |
| `grbljs_arc-xz-plane.gcode` | `cncjs/cncjs/examples/gcode/arc-xz-plane.gcode` | 37 | `91c48b24aeb1` |
| `grbljs_arc-yz-plane.gcode` | `cncjs/cncjs/examples/gcode/arc-yz-plane.gcode` | 37 | `95b4c1a00abe` |
| `grbljs_circle-inch.gcode` | `cncjs/cncjs/examples/gcode/circle-inch.gcode` | 171 | `1e24bdf0955f` |
| `grbljs_circle.gcode` | `cncjs/cncjs/examples/gcode/circle.gcode` | 135 | `510f79ab4f4f` |
| `grbljs_full-circle-without-xyz.gcode` | `cncjs/cncjs/examples/gcode/full-circle-without-xyz.gcode` | 266 | `23c903f1202f` |
| `grbljs_g2-arc.gcode` | `cncjs/cncjs/examples/gcode/g2-arc.gcode` | 102 | `087814e84717` |
| `grbljs_helical-thread-milling.gcode` | `cncjs/cncjs/examples/gcode/helical-thread-milling.gcode` | 455 | `a3fce3c71673` |
| `grbljs_jsdc-laser.gcode` | `cncjs/cncjs/examples/gcode/jsdc-laser.gcode` | 52771 | `402b4808b6b6` |
| `grbljs_jsdc.gcode` | `cncjs/cncjs/examples/gcode/jsdc.gcode` | 53031 | `7256e886ccc2` |
| `grbljs_sample.gcode` | `cncjs/cncjs/examples/gcode/sample.gcode` | 538699 | `46e39d814408` |

---

## Источник 3+ (итерация 4, добор published-контура): 20 файлов, 7 под-источников, все MIT

Дата: 2026-11-20. Отбор: GitHub repo-search с license:mit/noassertion-
фильтром; лицензии каждой репо проверены GitHub API (license.spdx_id).
FluidNC (NOASSERTION), grblHAL (no license), gnea/grbl (NOASSERTION) —
НЕ пригодны для published, не собирались (см. отчёт).

### sameer/svg2gcode — MIT (3 файл.)

- URL базы: https://github.com/sameer/svg2gcode
- Дата: 2026-11-20; лицензия: MIT (GitHub API license — проверена).
- Диалект-предположение: grbl-предположение (IJK-дуги, обычный словарь).
- Примечание: svg→gcode конвертер (452★); тестовые файлы конвертера.

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `mit_svg2g_circular_interpolation.gcode` | `sameer/svg2gcode/g_code/tests/circular_interpolation.gcode` | 2166 | `1505503d1281` |
| `mit_svg2g_shapes.gcode` | `sameer/svg2gcode/g_code/tests/shapes.gcode` | 24152 | `865c6a7200b1` |
| `mit_svg2g_square.gcode` | `sameer/svg2gcode/g_code/tests/square.gcode` | 1074 | `260ae3aea04b` |

### Nikolay-Kha/PyCNC — MIT (3 файл.)

- URL базы: https://github.com/Nikolay-Kha/PyCNC
- Дата: 2026-11-20; лицензия: MIT (GitHub API license — проверена).
- Диалект-предположение: grbl-подобный/собственный (простые G0/G1/G2/G3).
- Примечание: PyCNC (650★) — драйвер RPi CNC; тесты библиотеки.

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `mit_pycnc_circles.gcode` | `Nikolay-Kha/PyCNC/tests/circles.gcode` | 414 | `ca3ec1943c6e` |
| `mit_pycnc_rects.gcode` | `Nikolay-Kha/PyCNC/tests/rects.gcode` | 200 | `ff82c147066f` |
| `mit_pycnc_test_parser.gcode` | `Nikolay-Kha/PyCNC/tests/test_parser.gcode` | 311 | `4e940c76cf86` |

### TheSaturdayThing/shapeoko_arduino — MIT (2 файл.)

- URL базы: https://github.com/TheSaturdayThing/shapeoko_arduino
- Дата: 2026-11-20; лицензия: MIT (GitHub API license — проверена).
- Диалект-предположение: grbl (Shapeoko работает на GRBL).
- Примечание: Shapeoko-проект: логотип .nc (50KB), circle.nc (пропущен как content-dup c cncjs circle.gcode), drill-файл .tap.

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `mit_shpk_MIT_EC_Logo.nc` | `TheSaturdayThing/shapeoko_arduino/GCode/MIT EC Logo/MIT EC Logo.nc` | 50048 | `3f40f9e0382d` |
| `mit_shpk_ncdrill1.tap` | `TheSaturdayThing/shapeoko_arduino/Electronics/shields/buildlog shield/gerber/ncdrill1.tap` | 1815 | `0a21ab855f24` |

### filipecaixeta/cncwebsim — MIT (2 файл.)

- URL базы: https://github.com/filipecaixeta/cncwebsim
- Дата: 2026-11-20; лицензия: MIT (GitHub API license — проверена).
- Диалект-предположение: grbl (web-симулятор GRBL-подобного словаря).
- Примечание: web-симулятор: fresa (фреза), torno (токарный) — испанские программы.

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `mit_cws_fresa.gcode` | `filipecaixeta/cncwebsim/gcode/fresa.gcode` | 4949 | `bee648eddcb7` |
| `mit_cws_torno.gcode` | `filipecaixeta/cncwebsim/gcode/torno.gcode` | 3791 | `29fb9df4965c` |

### sliptonic/hilbert_cnc — MIT (1 файл.)

- URL базы: https://github.com/sliptonic/hilbert_cnc
- Дата: 2026-11-20; лицензия: MIT (GitHub API license — проверена).
- Диалект-предположение: универсальный G0/G1 (автор — контрибьютор LinuxCNC/QTdragon).
- Примечание: hilbert-кривая 123KB — длинная реальная программа.

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `mit_hilbert_test.ngc` | `sliptonic/hilbert_cnc/test.ngc` | 123041 | `c3b75f3e724a` |

### ErwinRieger/ddprint-test-gcode — MIT (6 файл.)

- URL базы: https://github.com/ErwinRieger/ddprint-test-gcode
- Дата: 2026-11-20; лицензия: MIT (GitHub API license — проверена).
- Диалект-предположение: marlin-3dp (E-слова экструзии — вне GRBL/LCNC словаря → ожидаемо диагностика; robustness-материал).
- Примечание: тестовые файлы ddprint: dwell, simple_square, m_retract, buchse, eck3, ltr.

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `mit_ddp_buchse.gcode` | `ErwinRieger/ddprint-test-gcode/test_files/buchse.gcode` | 1166 | `4bb2ca85e6cf` |
| `mit_ddp_dwell.gcode` | `ErwinRieger/ddprint-test-gcode/test_files/dwell.gcode` | 83 | `4c7be72c2ec6` |
| `mit_ddp_eck3.gcode` | `ErwinRieger/ddprint-test-gcode/test_files/eck3.gcode` | 3319 | `f639ce6728a0` |
| `mit_ddp_ltr.gcode` | `ErwinRieger/ddprint-test-gcode/test_files/ltr.gcode` | 368 | `1d71ddd66e13` |
| `mit_ddp_m_retract.gcode` | `ErwinRieger/ddprint-test-gcode/test_files/m_retract.gcode` | 68 | `e85e8ad15b2f` |
| `mit_ddp_simple_square.gcode` | `ErwinRieger/ddprint-test-gcode/test_files/simple_square.gcode` | 263 | `07271ef17d44` |

### cncjs/cncjs (examples/gcode/tball) — MIT (3 файл.)

- URL базы: https://github.com/cncjs/cncjs (examples/gcode/tball)
- Дата: 2026-11-20; лицензия: MIT (GitHub API license — проверена).
- Диалект-предположение: grbl (хост cncjs).
- Примечание: tball-программы: гравировка/карман (pocket >600KB — отброшены 2).

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `mit_tball_tball-engrave-laser.gcode` | `cncjs/cncjs/examples/gcode/tball/tball-engrave-laser.gcode` | 77416 | `d8a8fdd133a8` |
| `mit_tball_tball-engrave.gcode` | `cncjs/cncjs/examples/gcode/tball/tball-engrave.gcode` | 77543 | `46b42bba579d` |
| `mit_tball_tball.gcode` | `cncjs/cncjs/examples/gcode/tball/tball.gcode` | 61508 | `61dfc201991f` |

---

## Источник 4 (итерация 5, diversity batch 2): 29 файлов, 4 под-источника (26 MIT + 3 Apache-2.0)

Дата: 2026-11-21. Сборщик: Агент D (автоматический, GitHub API +
raw.githubusercontent.com); provenance-маппинг, license-ревью и внесение
в manifest.toml — оркестратор. Diversity-чек-лист (CORPUS §7): закрыты
«pocketing с Z-проходами», «3D-контуры/Z-уровни», «радиусные контуры»,
«drill-grid A/B-паттерны»; 2D-skew первой партии снят.

Лицензии проверены GitHub API `/repos/<repo>/license` (2026-11-21):
**MIT** — mrRobot62/gcodebuilder, ollema/gcode, vincedovy/CNC-files;
**Apache-2.0** — Jackhammer88/Visu. Байт-верификация маппинга: 8/29
скачаны повторно (raw.githubusercontent, HEAD) — sha256 совпал с
локальным; остальные 21 — точное имя+размер в дереве репо + совпадение
с черновиками сбора (`_diversity_cands3.txt`).

Диалект-разметка (по содержимому, приёмка manifest): **linuxcnc** —
`gcb_*` (G64+M6 в контенте), `olle_*` (G91.1 + `;`-комментарии);
**grbl-1.1** — `vince_*` (FreeCAD-пост «grbl_post»), `vis_*` (M7,
T-предвыбор). Отброшено при сборе: FluidNC/grblHAL/gnea-grbl
(NOASSERTION/no license), `pocketRRect_#13_001.gcode` (`#` в имени),
файлы >600KB.

### mrRobot62/gcodebuilder — MIT (16 файлов)

- URL базы: https://github.com/mrRobot62/gcodebuilder (app/gcode)
- Дата: 2026-11-21; лицензия: MIT (GitHub API — проверена).
- Диалект: linuxcnc (CAM-пост: G64/G40/G41/G42, M6).
- Примечание: 3 файла (90deg, ContourRect_G40/G41) — имена нормализованы
  сборщиком, маппинг байт-верифицирован.

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `gcb_ContourHolesOnGrid_4x2y_A30_B20_22.5deg.ngc` | `app/gcode/ContourHolesOnGrid_4x2y_A30_B20_22.5deg.ngc` | 2404 | `b715091094d7` |
| `gcb_ContourHolesOnGrid_90deg.ngc` | `app/gcode/ContourHolesOnGrid_4x2y_A30_B20_90deg.ngc` | 2404 | `031d63dea4a9` |
| `gcb_ContourRect_G40.ngc` | `app/gcode/ContourRect_TD3.0_A20_B30_D-2.5_G40.ngc` | 2160 | `1b0507a75714` |
| `gcb_ContourRect_G41.ngc` | `app/gcode/ContourRect_TD3.0_A20_B30_D-2.5_G41.ngc` | 2165 | `2379c0d55520` |
| `gcb_ContourRoundRect_A70B100_G42_TD-8.75.gcode` | `app/gcode/ContourRoundRect_A70B100_G42_TD-8.75.gcode` | 8939 | `2d9538d0ed2a` |
| `gcb_TEST_DrillHolesGrid_45deg_T4.ngc` | `app/gcode/TEST_DrillHolesGrid_45deg_T4.ngc` | 1999 | `74e0b43bd152` |
| `gcb_TEST_MillHolesGrid_12.5deg_10.5depth_T4.ngc` | `app/gcode/TEST_MillHolesGrid_12.5deg_10.5depth_T4.ngc` | 54217 | `ff7f65cad36f` |
| `gcb_TEST_MillPocketCircle_SO30.ngc` | `app/gcode/TEST_MillPocketCircle_SO30.ngc` | 3148 | `b53a6d955a9a` |
| `gcb_TEST_MillPocketCircle_SO50.ngc` | `app/gcode/TEST_MillPocketCircle_SO50.ngc` | 3892 | `2e5d55a8a04a` |
| `gcb_contourMillHolesGrid_001.gcode.ngc` | `app/gcode/contourMillHolesGrid_001.gcode.ngc` | 11704 | `ddcfa55ea7b6` |
| `gcb_drillHolesGrid_A10B10_3x3_tD6_45.gcode` | `app/gcode/drillHolesGrid_A10B10_3x3_tD6_45.gcode` | 2278 | `25b8dca1821d` |
| `gcb_drillHoles_R100_nH4_tD-2.5_0.00deg.gcode` | `app/gcode/drillHoles_R100_nH4_tD-2.5_0.00deg.gcode` | 3070 | `0758cf1dd2a5` |
| `gcb_pocketCircle_oR30_iR0_T6_tD-2.5.gcode` | `app/gcode/pocketCircle_oR30_iR0_T6_tD-2.5.gcode` | 6220 | `ef23fd720f5b` |
| `gcb_pocketRRect_001_02.gcode` | `app/gcode/pocketRRect_001_02.gcode` | 13280 | `ab6a91e5af8d` |
| `gcb_pocketRRect_001a.gcode` | `app/gcode/pocketRRect_001a.gcode` | 748 | `ea8557c6f391` |
| `gcb_spiral.gcode` | `app/gcode/spiral.gcode` | 9175 | `15d575e33811` |

### ollema/gcode — MIT (5 файлов)

- URL базы: https://github.com/ollema/gcode
- Дата: 2026-11-21; лицензия: MIT (GitHub API — проверена).
- Диалект: linuxcnc (G91.1, `;`-комментарии, G54).
- Примечание: .tap CNC-программы ( bricka/ivar/top — 3D-контур с Z-проходами).

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `olle_bricka_short_wall.tap` | `bricka short wall.tap` | 7943 | `dd3ffe536b5d` |
| `olle_bricka_slots.tap` | `bricka slots.tap` | 222 | `88c1618c2e7c` |
| `olle_ivar_engraving_test.tap` | `ivar engraving test.tap` | 31281 | `5e27fd811164` |
| `olle_top_cont.tap` | `top cont.tap` | 6112 | `900df1ec5893` |
| `olle_top_trace_finish.tap` | `top trace finish.tap` | 2832 | `fd793092b590` |

### vincedovy/CNC-files — MIT (5 файлов)

- URL базы: https://github.com/vincedovy/CNC-files
- Дата: 2026-11-21; лицензия: MIT (GitHub API — проверена).
- Диалект: grbl-1.1 (FreeCAD-пост «grbl_post»).
- Примечание: 740KB/196KB варианты BewareOfDog (v10_1_T7) и BrushRabbit
  (_1_/_2_/_3_Engrave) отброшены размерным cap; cead-mile-failte.nc (227KB)
  — резерв.

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `vince_BewareOfDog_v10.nc` | `BewareOfDog v10.nc` | 379 | `d7fbe5360fad` |
| `vince_BewareOfDog_v10_2_T2.nc` | `BewareOfDog v10_2_T2.nc` | 9513 | `ab6bc8bbcbda` |
| `vince_BewareOfDog_v10_3_T4.nc` | `BewareOfDog v10_3_T4.nc` | 181782 | `9b924c36b899` |
| `vince_BrushRabbitv4.nc` | `BrushRabbitv4.nc` | 307 | `9e4eaddab0f4` |
| `vince_tictactoe1.nc` | `tictactoe1.nc` | 33437 | `8428014cc354` |

### Jackhammer88/Visu — Apache-2.0 (3 файла)

- URL базы: https://github.com/Jackhammer88/Visu (Visu/GcodeExamples)
- Дата: 2026-11-21; лицензия: Apache-2.0 (GitHub API — проверена).
- Диалект: grbl-1.1 (GRBL-визуализатор; M7, T-предвыбор).
- Примечание: SmilinJack relief 3.5MB — крупный 3D-рельеф (Z-уровни).

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
|---|---|---|---|
| `vis_SmilinJack_-_ThreeAxisFlatMachine_Relief.tap` | `Visu/GcodeExamples/SmilinJack - ThreeAxisFlatMachine Relief.tap` | 3540057 | `dcefe47a28c2` |
| `vis_hse1p.tap` | `Visu/GcodeExamples/hse1p.tap` | 13011 | `9a14a964bee2` |
| `vis_knight.tap` | `Visu/GcodeExamples/knight.tap` | 142599 | `88abd1eaa71b` |

---

## Источник 5 (итерация 6, конструкционно-целевой добор): 23 файла, 5 под-источников (8 MIT + 6 GPL-2.0 + 4 GPL-3.0 + 5 NOASSERTION)

Дата: 2026-11-21. Сборщик: агент сбора; license-ревью и приёмка — оркестратор
(лицензии независимо перепроверены GitHub API `/repos/<repo>/license`:
MIT — 0BAB1/gcodeEstimator, cnc4me/macro-programming-examples; GPL-2.0 —
LinuxCNC; GPL-3.0 — ismael-figueiredo/cnc-simulator; **NOASSERTION** —
CNC-Master-xyz/NC-Code: /license 404, LICENSE-файла нет; первичный
grep-выдача «MIT» была ошибкой /search/repositories — сдвиг пар,
перепроверено точечным запросом). Полный отчёт сбора с per-file таблицами:
`_construct_collection.md`.

Цель добора — **конструкционное покрытие** (2–3 файла на конструкцию), не
контент-разнообразие: G33/G33.1/G84/G74, G76 ×4 диалекта, G87/G88,
G65/M98/M99/Macro B, G41-D-word, комбинированные `G43 H# Z#.`, G69,
G43.4-RTCP, G12.1/G13.1, G15-lathe, M70/M72/M73/M61, Fanuc-ловушки
(`;`-EOB, bare-integer DPI, G10L2P1Z-беспробельно, `%`+O, T0101-пары).
Итог: 7/11 конструкций закрыты корпусом, 3 частично, список сирот
(конструкции без публичных сэмплов вообще) — §4 `_construct_collection.md`.

### 0BAB1/gcodeEstimator — MIT (4 файла) → inbox

- URL: https://github.com/0BAB1/gcodeEstimator (tests/g_test_files)
- Диалект: fanuc (TEST_G74/G76/G71/full — lathe-программы с EOB, DPI).

### cnc4me/macro-programming-examples — MIT (4 файла) → inbox

- URL: https://github.com/cnc4me/macro-programming-examples (programs/, examples/)
- Диалект: fanuc Macro B (G65+args, IF/GOTO, WHILE, Renishaw-стиль probe).

### LinuxCNC/linuxcnc — GPL-2.0 (6 файлов) → local

- URL: https://github.com/LinuxCNC/linuxcnc (nc_files/, docs/)
- Диалект: linuxcnc. G33.1-файл — verbatim-выдержка документации (5 строк).

### ismael-figueiredo/cnc-simulator — GPL-3.0 (4 файла) → local

- URL: https://github.com/ismael-figueiredo/cnc-simulator (examples/)
- Диалект: fanuc (ROMI GL300 lathe, Haas VF2 mill) — `;`-EOB, G154 P,
  комбинированные `G43 Z#. H#`.

### CNC-Master-xyz/NC-Code — NOASSERTION (5 файлов) → local (license unclear — local only)

- URL: https://github.com/CNC-Master-xyz/NC-Code (Fanuc/, Haas/)
- Диалект: fanuc/haas (2.5D mill, 3D 5X, RTCP 5X, TURN_1, TURN).
- **Cap-исключения (приняты оркестратором):** `ncm_g434_rtcp_5x_milling.nc`
  (2.1MB — единственный публичный RTCP-сэмпл в GitHub) и
  `ncm_g43_g69_3d5x_milling.nc` (2.5MB — G69 5X) — единственные источники
  конструкций G43.4/G69; local-контур, не публикуются.

---

## Источник 6 (итерация 9, haas-добор): 13 файлов, 6 под-источников (1 MIT + 12 NOASSERTION)

Дата: 2026-11-22. Сборщик: агент сбора; license-ревью и приёмка — оркестратор
(GitHub API независимо: cnc4me 200/MIT; zodiaceng/olafgromotka 404 — NOASSERTION
подтверждён). **Пермиссивного живого Haas-NGC на GitHub нет** (~15 repo-поисков:
авторские бэкапы без лицензии; CC0-репо отбракован как LLM-псевдо-G-code).
Полный отчёт: `_haas_collection.md`.

Цель — конструкции eid-haas: P-dot-rule (dot ×6 / bare ×18), Macro B #500–#699
(#598 ×66 хитов, THEN, bit-addressing), **G31-сирота итерации 8 закрыта**
(7 вызовов), **H-до-Z порядок найден** (`G43 H33 Z2.`, ранее «не найден»),
G154-негативные границы (P10/P11/P98 → Raw вне p-домена), M59/M96-семейство,
ловушки (`G103P1` без пробела, unclosed-paren, Q-шум в G81).

Приёмка: **1 MIT → inbox** (mpe_); **12 NOASSERTION → local** (gitignored,
CORPUS §7). manifest: **dialect = "haas"** — KNOWN_DIALECTS harness-теста
расширен (4→5); live через HaasAdapter в full_registry. Сироты: G83-I/J/K и
G73-mill в диком виде не найдены (только tutorials ©/Cloudflare).

### cnc4me/macro-programming-examples — MIT (1 файл) → inbox

- URL: https://github.com/cnc4me/macro-programming-examples
- Диалект: haas (Macro B #540/#598, toolpath profiling).

### NOASSERTION (12 файлов) → local (license unclear — local only)

zodiaceng/haas-mill-programs (1), ZapCon1/HaasMacros (3), vulture2600/NPT-Macro (2),
coyoinu/HaasBAxisTram (1), olafgromotka/HAAS-NGC-Renishaw-9000-programs (5,
вендорский Renishaw-контент — минимальный достаток).

---

## Сводка

- **Контуры (ruling CORPUS v0.6.4):** published (inbox) = **69 файлов**
  (66 MIT + 3 Apache-2.0); local-dev (tier_b/local) = **75 файлов**
  (GPL-2.0/GPL-3.0/NOASSERTION — не публикуются). Итого manifest: **144**
  (из них dialect=fanuc: 17 — Pending до регистрации eid-fanuc в
  adapter-реестре).
- Конструкционное покрытие (iter-6 добор): 7/11 закрыто, 3 частично,
  1 (G5/G6-сплайны вне LinuxCNC-NURBS) — без публичных источников.
- published: MIT 66 + Apache-2.0 3; local-dev: GPL-2.0 66 + GPL-3.0 4 +
  NOASSERTION 5.
- Контент-профили для приёмки: LinuxCNC — вендор-примеры + тесты
  интерпретатора (tests/interp содержит параметрику/OWord — часть файлов
  ожидаемо даст E_UNSUPPORTED_SYNTAX/RUNTIME_STATE — штатный parse-or-
  diagnose материал Tier B); cncjs — GRBL-семплы (дуги/окружности/лазер).