# REPORT-ITER23 — полный прогон корпуса через eid-harness runner

- прогон: контрольная точка итераций 2–3 (Агент B); NOTHING FIXED — сырые расхождения для классификации оркестратором (CORPUS §9)
- corpus: `v0.1-pilot` (векторов: 68)
- адаптеры: `eid-l1` = EidTextAdapter, `grbl-1.1` = GrblAdapter, `linuxcnc` = LinuxCncAdapter; R4 — translate_checked/translate_checked_golden (CORPUS v0.5.3)
- сводка runner: Pass = 326, Fail = 13, Pending = 0, Skipped = 2

## Счётчики по отношениям

| Отношение | Pass | Fail | Pending | Skipped |
|---|---:|---:|---:|---:|
| R1 | 48 | 0 | 0 | 0 |
| R2 | 48 | 0 | 0 | 0 |
| R3-golden | 69 | 11 | 0 | 0 |
| R3-consistency | 33 | 0 | 0 | 0 |
| R1-text | 7 | 0 | 0 | 0 |
| R4 | 8 | 0 | 0 | 2 |
| parse | 113 | 2 | 0 | 0 |

## Покрытие R3 (CORPUS v0.6)

R3 (any-to-any) доказывается ТОЛЬКО на материализованном подмножестве: **R3 на 48 из 48 позитивных** CORPUS §1.1.

Отсутствующие 0 векторов:


## Не-Pass позиции (каждый вектор, каждая непрошедшая проверка)

### 17 — drill_G81

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/17_drill_G81/drill_G81.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 6 divergences vs golden: [OpCount { a: 6, b: 5 }, Geometry { index: 1, field: "end" }, StateField { index: 1, field: "feed" }, OpKind { index: 2 }, OpKind { index: 3 }, OpKind { index: 4 }]

### 18 — drill_G82_dwell

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/18_drill_G82_dwell/drill_G82_dwell.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 7 divergences vs golden: [OpCount { a: 7, b: 6 }, Geometry { index: 1, field: "end" }, StateField { index: 1, field: "feed" }, OpKind { index: 2 }, OpKind { index: 3 }, OpKind { index: 4 }, OpKind { index: 5 }]

### 19 — peck_G83_retract

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/19_peck_G83_retract/peck_G83_retract.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 18 divergences vs golden: [OpCount { a: 15, b: 14 }, Geometry { index: 1, field: "end" }, StateField { index: 1, field: "feed" }, OpKind { index: 2 }, OpKind { index: 3 }, Geometry { index: 4, field: "start" }, Geometry { index: 4, field: "end" }, OpKind { index: 5 }, OpKind { index: 6 }, Geometry { index: 7, field: "start" }, Geometry { index: 7, field: "end" }, OpKind { index: 8 }, OpKind { index: 9 }, Geometry { index: 10, field: "start" }, Geometry { index: 10, field: "end" }, OpKind { index: 11 }, OpKind { index: 12 }, OpKind { index: 13 }]

### 20 — chipbreak_G73

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/20_chipbreak_G73/chipbreak_G73.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 14 divergences vs golden: [OpCount { a: 12, b: 11 }, Geometry { index: 1, field: "end" }, StateField { index: 1, field: "feed" }, OpKind { index: 2 }, OpKind { index: 3 }, Geometry { index: 4, field: "start" }, Geometry { index: 4, field: "end" }, OpKind { index: 5 }, OpKind { index: 6 }, Geometry { index: 7, field: "start" }, Geometry { index: 7, field: "end" }, OpKind { index: 8 }, OpKind { index: 9 }, OpKind { index: 10 }]

### 21 — bore_G86_restart

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/21_bore_G86_restart/bore_G86_restart.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 7 divergences vs golden: [OpCount { a: 7, b: 6 }, Geometry { index: 1, field: "end" }, StateField { index: 1, field: "feed" }, OpKind { index: 2 }, OpKind { index: 3 }, OpKind { index: 4 }, OpKind { index: 5 }]

### 22 — bore_G89_dwell

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/22_bore_G89_dwell/bore_G89_dwell.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 7 divergences vs golden: [OpCount { a: 7, b: 6 }, Geometry { index: 1, field: "end" }, StateField { index: 1, field: "feed" }, OpKind { index: 2 }, OpKind { index: 3 }, OpKind { index: 4 }, OpKind { index: 5 }]

### 23 — cycle_G99

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/23_cycle_G99/cycle_G99.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 6 divergences vs golden: [OpCount { a: 6, b: 5 }, Geometry { index: 1, field: "end" }, StateField { index: 1, field: "feed" }, OpKind { index: 2 }, OpKind { index: 3 }, OpKind { index: 4 }]

### 24 — cycle_G98

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/24_cycle_G98/cycle_G98.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 6 divergences vs golden: [OpCount { a: 6, b: 5 }, Geometry { index: 1, field: "end" }, StateField { index: 1, field: "feed" }, OpKind { index: 2 }, OpKind { index: 3 }, OpKind { index: 4 }]

### 25 — cycle_repeat_distinct_RZ

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/25_cycle_repeat_distinct_RZ/cycle_repeat_distinct_RZ.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 10 divergences vs golden: [OpCount { a: 10, b: 8 }, Geometry { index: 1, field: "end" }, StateField { index: 1, field: "feed" }, OpKind { index: 2 }, OpKind { index: 3 }, Geometry { index: 4, field: "start" }, Geometry { index: 4, field: "end" }, OpKind { index: 5 }, Geometry { index: 6, field: "start" }, OpKind { index: 7 }]

### 26 — cycle_cancel_G80

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/26_cycle_cancel_G80/cycle_cancel_G80.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 7 divergences vs golden: [OpCount { a: 7, b: 6 }, Geometry { index: 1, field: "end" }, StateField { index: 1, field: "feed" }, OpKind { index: 2 }, OpKind { index: 3 }, OpKind { index: 4 }, OpKind { index: 5 }]

### 47 — raw_crlf_fidelity

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/47_raw_crlf_fidelity/raw_crlf_fidelity.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 1 divergences vs golden: [RawMismatch { index: 0 }]

### N3 — raw_vendor_block

- `R4` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n3_raw_vendor_block/raw_vendor_block.grbl.ngc→grbl-1.1` | ожидалось: emit отказ E_RAW_UNTRANSLATABLE | фактически: Skipped: same-dialect пара: RAW-эмиссия легальна (verbatim, CORPUS v0.5.3)
- `R4` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n3_raw_vendor_block/raw_vendor_block.linuxcnc.ngc→linuxcnc` | ожидалось: emit отказ E_RAW_UNTRANSLATABLE | фактически: Skipped: same-dialect пара: RAW-эмиссия легальна (verbatim, CORPUS v0.5.3)

### 48 — arc_centers_mode_distinct

- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/48_arc_centers_mode_distinct/arc_centers_mode_distinct.grbl.ngc` | ожидалось: parse Ok | фактически: Fail: expected ok, refused: E_INVALID_INPUT
- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/48_arc_centers_mode_distinct/arc_centers_mode_distinct.linuxcnc.ngc` | ожидалось: parse Ok | фактически: Fail: expected ok, refused: E_INVALID_INPUT
