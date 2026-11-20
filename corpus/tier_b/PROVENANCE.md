# Tier B — provenance-журнал входящих файлов (CORPUS §7)

Дата сбора: 2026-11-20. Сборщик: Агент D (автоматический, GitHub API +
raw.githubusercontent.com). Метод дедупликации: sha256 по содержимому —
в итоговом inbox **72 файла, дубликатов содержимого: 0** (1 дубль cncjs/
LinuxCNC-семпла пропущен на приёме). Файлы лежат в `corpus/tier_b/inbox/`.
В manifest.toml НЕ внесены, не парсились — приёмка в корпус после
проверки лицензий оркестратором (CORPUS §7).

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
- СТАТУС ЛИЦЕНЗИИ: **сомнительный** — GPL-2.0 + неясное per-file
  авторство (большинство файлов — вклады участников без явных заголовков).
  Требует решения оркестратора (GPL-совместимость приёма в корпус).
- Каталоги выборки: nc_files/ (корень), ngcgui_lib (+lathe), macros/,
  probe/basic_probe/macros/, remap-subroutines/, nurbs/G5/ (полностью),
  tests/trajectory-planner/circular-arcs/nc_files/quick-tests/ + arc-feed/,
  tests/interp/good/, configs/sim/gmoccapy/macros/.

| Файл в inbox | Путь в репо | Байт | sha256 (12) |
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

## Сводка

- Всего файлов в inbox: **72** (LinuxCNC 60 + cncjs/GRBL 12), содержимо уникальны (sha256).
- Диалект-разбивка: linuxcnc-предположение 60, grbl-предположение 12.
- Лицензии: MIT — 12 файлов (чистые); GPL-2.0/per-file неизвестно — 60 файлов (сомнительные — вся секция Источник 1).
- Контент-профили для приёмки: LinuxCNC — вендор-примеры + тесты
  интерпретатора (tests/interp содержит параметрику/OWord — часть файлов
  ожидаемо даст E_UNSUPPORTED_SYNTAX/RUNTIME_STATE — штатный parse-or-
  diagnose материал Tier B); cncjs — GRBL-семплы (дуги/окружности/лазер).