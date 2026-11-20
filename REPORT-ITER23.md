# REPORT-ITER23 — полный прогон корпуса через eid-harness runner

- прогон: контрольная точка итераций 2–3 (Агент B); NOTHING FIXED — сырые расхождения для классификации оркестратором (CORPUS §9)
- corpus: `v0.1-pilot` (векторов: 57)
- адаптеры: `eid-l1` = EidTextAdapter, `grbl-1.1` = GrblAdapter, `linuxcnc` = LinuxCncAdapter; R4 — translate_checked/translate_checked_golden (CORPUS v0.5.3)
- сводка runner: Pass = 211, Fail = 70, Pending = 0, Skipped = 2

## Счётчики по отношениям

| Отношение | Pass | Fail | Pending | Skipped |
|---|---:|---:|---:|---:|
| R1 | 37 | 0 | 0 | 0 |
| R2 | 37 | 0 | 0 | 0 |
| R3-golden | 28 | 34 | 0 | 0 |
| R3-consistency | 1 | 26 | 0 | 0 |
| R1-text | 7 | 0 | 0 | 0 |
| R4 | 8 | 0 | 0 | 2 |
| parse | 93 | 10 | 0 | 0 |

## Покрытие R3 (CORPUS v0.6)

R3 (any-to-any) доказывается ТОЛЬКО на материализованном подмножестве: **R3 на 37 из 47 позитивных** CORPUS §1.1.

Отсутствующие 10 векторов:

- #17 `drill_G81` — цикловые (развёртывание SPEC §7) — итерация 3–4.
- #18 `drill_G82_dwell` — цикловые (развёртывание SPEC §7) — итерация 3–4.
- #19 `peck_G83_retract` — цикловые (развёртывание SPEC §7) — итерация 3–4.
- #20 `chipbreak_G73` — цикловые (развёртывание SPEC §7) — итерация 3–4.
- #21 `bore_G86_restart` — цикловые (развёртывание SPEC §7) — итерация 3–4.
- #22 `bore_G89_dwell` — цикловые (развёртывание SPEC §7) — итерация 3–4.
- #23 `cycle_G99` — цикловые (развёртывание SPEC §7) — итерация 3–4.
- #24 `cycle_G98` — цикловые (развёртывание SPEC §7) — итерация 3–4.
- #25 `cycle_repeat_distinct_RZ` — цикловые (развёртывание SPEC §7) — итерация 3–4.
- #26 `cycle_cancel_G80` — цикловые (развёртывание SPEC §7) — итерация 3–4.

## Не-Pass позиции (каждый вектор, каждая непрошедшая проверка)

### 01 — linear_box

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/01_linear_box/linear_box.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/01_linear_box/linear_box.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/01_linear_box/linear_box.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 02 — rapid_reposition

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/02_rapid_reposition/rapid_reposition.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/02_rapid_reposition/rapid_reposition.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/02_rapid_reposition/rapid_reposition.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 03 — feed_survives_rapid

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/03_feed_survives_rapid/feed_survives_rapid.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/03_feed_survives_rapid/feed_survives_rapid.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/03_feed_survives_rapid/feed_survives_rapid.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 11 — arc_R_semicircle

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/11_arc_R_semicircle/arc_R_semicircle.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/11_arc_R_semicircle/arc_R_semicircle.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/11_arc_R_semicircle/arc_R_semicircle.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 36 — preset_G54_then_switch

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/36_preset_G54_then_switch/preset_G54_then_switch.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/36_preset_G54_then_switch/preset_G54_then_switch.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/36_preset_G54_then_switch/preset_G54_then_switch.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 46 — arc_R_helix

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/46_arc_R_helix/arc_R_helix.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/46_arc_R_helix/arc_R_helix.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/46_arc_R_helix/arc_R_helix.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 04 — feed_G94_basic

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/04_feed_G94_basic/feed_G94_basic.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/04_feed_G94_basic/feed_G94_basic.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/04_feed_G94_basic/feed_G94_basic.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 05 — feed_G93_inverse

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/05_feed_G93_inverse/feed_G93_inverse.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/05_feed_G93_inverse/feed_G93_inverse.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/05_feed_G93_inverse/feed_G93_inverse.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 06 — dwell_between_moves

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/06_dwell_between_moves/dwell_between_moves.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/06_dwell_between_moves/dwell_between_moves.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/06_dwell_between_moves/dwell_between_moves.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 07 — arc_90

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/07_arc_90/arc_90.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/07_arc_90/arc_90.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/07_arc_90/arc_90.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 08 — arc_full_circle

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/08_arc_full_circle/arc_full_circle.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/08_arc_full_circle/arc_full_circle.linuxcnc.ngc` | ожидалось: parse Ok | фактически: Fail: expected ok, refused: E_INVALID_INPUT

### 09 — arc_helix

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/09_arc_helix/arc_helix.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/09_arc_helix/arc_helix.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/09_arc_helix/arc_helix.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 10 — arc_helix_inch

- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/10_arc_helix_inch/arc_helix_inch.grbl.ngc` | ожидалось: parse Ok | фактически: Fail: expected ok, refused: E_UNSUPPORTED_SEMANTICS

### 12 — arc_R_sweep270

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/12_arc_R_sweep270/arc_R_sweep270.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/12_arc_R_sweep270/arc_R_sweep270.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/12_arc_R_sweep270/arc_R_sweep270.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 13 — arc_degenerate_zero_sweep

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/13_arc_degenerate_zero_sweep/arc_degenerate_zero_sweep.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 4 divergences vs golden: [MetaField { field: "path_mode" }, OpCount { a: 4, b: 3 }, OpKind { index: 1 }, OpKind { index: 2 }]
- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/13_arc_degenerate_zero_sweep/arc_degenerate_zero_sweep.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 3 divergences vs golden: [OpCount { a: 4, b: 3 }, OpKind { index: 1 }, OpKind { index: 2 }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/13_arc_degenerate_zero_sweep/arc_degenerate_zero_sweep.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/13_arc_degenerate_zero_sweep/arc_degenerate_zero_sweep.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 14 — arc_G18

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/14_arc_G18/arc_G18.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/14_arc_G18/arc_G18.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/14_arc_G18/arc_G18.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 15 — arc_G19

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/15_arc_G19/arc_G19.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/15_arc_G19/arc_G19.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/15_arc_G19/arc_G19.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 16 — plane_mix

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/16_plane_mix/plane_mix.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/16_plane_mix/plane_mix.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/16_plane_mix/plane_mix.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 47 — raw_crlf_fidelity

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/47_raw_crlf_fidelity/raw_crlf_fidelity.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 4 divergences vs golden: [MetaField { field: "path_mode" }, OpKind { index: 0 }, OpKind { index: 1 }, OpKind { index: 2 }]

### 28 — stops

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/28_stops/stops.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/28_stops/stops.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/28_stops/stops.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 29 — coolant_mist_flood

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/29_coolant_mist_flood/coolant_mist_flood.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/29_coolant_mist_flood/coolant_mist_flood.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/29_coolant_mist_flood/coolant_mist_flood.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 30 — spindle_reversal

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/30_spindle_reversal/spindle_reversal.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/30_spindle_reversal/spindle_reversal.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/30_spindle_reversal/spindle_reversal.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 31 — wcs_switch_mid

- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/31_wcs_switch_mid/wcs_switch_mid.grbl.ngc` | ожидалось: parse Ok | фактически: Fail: expected ok, refused: E_RUNTIME_STATE
- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/31_wcs_switch_mid/wcs_switch_mid.linuxcnc.ngc` | ожидалось: parse Ok | фактически: Fail: expected ok, refused: E_RUNTIME_STATE

### 32 — wcs_define_mid

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/32_wcs_define_mid/wcs_define_mid.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/32_wcs_define_mid/wcs_define_mid.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/32_wcs_define_mid/wcs_define_mid.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 33 — wcs_define_multi

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/33_wcs_define_multi/wcs_define_multi.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [MetaField { field: "path_mode" }, StateField { index: 4, field: "feed" }]
- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/33_wcs_define_multi/wcs_define_multi.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [StateField { index: 4, field: "feed" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/33_wcs_define_multi/wcs_define_multi.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/33_wcs_define_multi/wcs_define_multi.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 34 — preset_G54

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/34_preset_G54/preset_G54.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 3 divergences vs golden: [MetaField { field: "path_mode" }, StateField { index: 0, field: "spindle" }, StateField { index: 1, field: "spindle" }]
- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/34_preset_G54/preset_G54.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [StateField { index: 0, field: "spindle" }, StateField { index: 1, field: "spindle" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/34_preset_G54/preset_G54.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/34_preset_G54/preset_G54.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 35 — preset_G55

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/35_preset_G55/preset_G55.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 3 divergences vs golden: [MetaField { field: "path_mode" }, StateField { index: 0, field: "spindle" }, StateField { index: 1, field: "spindle" }]
- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/35_preset_G55/preset_G55.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [StateField { index: 0, field: "spindle" }, StateField { index: 1, field: "spindle" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/35_preset_G55/preset_G55.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/35_preset_G55/preset_G55.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 37 — g53_known_origins

- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/37_g53_known_origins/g53_known_origins.grbl.ngc` | ожидалось: parse Ok | фактически: Fail: expected ok, refused: E_RUNTIME_STATE
- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/37_g53_known_origins/g53_known_origins.linuxcnc.ngc` | ожидалось: parse Ok | фактически: Fail: expected ok, refused: E_RUNTIME_STATE

### 38 — units_inch

- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/38_units_inch/units_inch.grbl.ngc` | ожидалось: parse Ok | фактически: Fail: expected ok, refused: E_UNSUPPORTED_SEMANTICS

### 39 — modal_incremental

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/39_modal_incremental/modal_incremental.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/39_modal_incremental/modal_incremental.linuxcnc.ngc` | ожидалось: parse Ok | фактически: Fail: expected ok, refused: E_INVALID_INPUT

### 42 — minimal_program

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/42_minimal_program/minimal_program.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/42_minimal_program/minimal_program.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/42_minimal_program/minimal_program.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 43 — linear_zero_length_dropped

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/43_linear_zero_length_dropped/linear_zero_length_dropped.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [MetaField { field: "path_mode" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/43_linear_zero_length_dropped/linear_zero_length_dropped.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/43_linear_zero_length_dropped/linear_zero_length_dropped.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### 44 — wcs_set_here_preset

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/44_wcs_set_here_preset/wcs_set_here_preset.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 3 divergences vs golden: [MetaField { field: "path_mode" }, StateField { index: 1, field: "spindle" }, StateField { index: 2, field: "spindle" }]
- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/44_wcs_set_here_preset/wcs_set_here_preset.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [StateField { index: 1, field: "spindle" }, StateField { index: 2, field: "spindle" }]
- `R3-consistency` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/44_wcs_set_here_preset/wcs_set_here_preset.grbl.ngc×C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/44_wcs_set_here_preset/wcs_set_here_preset.linuxcnc.ngc` | ожидалось: parse(f_prev) ≡ parse(f_next) (semantic_diff = 0) | фактически: Fail: 1 divergences between dialect parses: [MetaField { field: "path_mode" }]

### N3 — raw_vendor_block

- `R4` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n3_raw_vendor_block/raw_vendor_block.grbl.ngc→grbl-1.1` | ожидалось: emit отказ E_RAW_UNTRANSLATABLE | фактически: Skipped: same-dialect пара: RAW-эмиссия легальна (verbatim, CORPUS v0.5.3)
- `R4` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n3_raw_vendor_block/raw_vendor_block.linuxcnc.ngc→linuxcnc` | ожидалось: emit отказ E_RAW_UNTRANSLATABLE | фактически: Skipped: same-dialect пара: RAW-эмиссия легальна (verbatim, CORPUS v0.5.3)

### N4 — peck_L_over_limit

- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n4_peck_L_over_limit/peck_L_over_limit.linuxcnc.ngc` | ожидалось: отказ E_LIMIT_EXCEEDED | фактически: Fail: expected refusal E_LIMIT_EXCEEDED, got E_UNSUPPORTED_SEMANTICS

### N6 — naked_coords_after_G80

- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n6_naked_coords_after_G80/naked_coords_after_G80.linuxcnc.ngc` | ожидалось: отказ E_INVALID_INPUT | фактически: Fail: expected refusal E_INVALID_INPUT, got E_UNSUPPORTED_SEMANTICS
