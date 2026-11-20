# REPORT-ITER23 — полный прогон корпуса через eid-harness runner

- прогон: контрольная точка итераций 2–3 (Агент B); NOTHING FIXED — сырые расхождения для классификации оркестратором (CORPUS §9)
- corpus: `v0.1-pilot` (векторов: 68)
- адаптеры: `eid-l1` = EidTextAdapter, `grbl-1.1` = GrblAdapter, `linuxcnc` = LinuxCncAdapter; R4 — translate_checked/translate_checked_golden (CORPUS v0.5.3)
- сводка runner: Pass = 332, Fail = 10, Pending = 0, Skipped = 2

## Счётчики по отношениям

| Отношение | Pass | Fail | Pending | Skipped |
|---|---:|---:|---:|---:|
| R1 | 48 | 0 | 0 | 0 |
| R2 | 48 | 0 | 0 | 0 |
| R3-golden | 72 | 10 | 0 | 0 |
| R3-consistency | 34 | 0 | 0 | 0 |
| R1-text | 7 | 0 | 0 | 0 |
| R4 | 8 | 0 | 0 | 2 |
| parse | 115 | 0 | 0 | 0 |

## Покрытие R3 (CORPUS v0.6)

R3 (any-to-any) доказывается ТОЛЬКО на материализованном подмножестве: **R3 на 48 из 48 позитивных** CORPUS §1.1.

Отсутствующие 0 векторов:


## Не-Pass позиции (каждый вектор, каждая непрошедшая проверка)

### 17 — drill_G81

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/17_drill_G81/drill_G81.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [StateField { index: 1, field: "feed" }, StateField { index: 2, field: "feed" }]

### 18 — drill_G82_dwell

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/18_drill_G82_dwell/drill_G82_dwell.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [StateField { index: 1, field: "feed" }, StateField { index: 2, field: "feed" }]

### 19 — peck_G83_retract

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/19_peck_G83_retract/peck_G83_retract.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 4 divergences vs golden: [StateField { index: 1, field: "feed" }, StateField { index: 2, field: "feed" }, Geometry { index: 5, field: "end" }, Geometry { index: 6, field: "start" }]

### 20 — chipbreak_G73

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/20_chipbreak_G73/chipbreak_G73.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [StateField { index: 1, field: "feed" }, StateField { index: 2, field: "feed" }]

### 21 — bore_G86_restart

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/21_bore_G86_restart/bore_G86_restart.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [StateField { index: 1, field: "feed" }, StateField { index: 2, field: "feed" }]

### 22 — bore_G89_dwell

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/22_bore_G89_dwell/bore_G89_dwell.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [StateField { index: 1, field: "feed" }, StateField { index: 2, field: "feed" }]

### 23 — cycle_G99

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/23_cycle_G99/cycle_G99.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [StateField { index: 1, field: "feed" }, StateField { index: 2, field: "feed" }]

### 24 — cycle_G98

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/24_cycle_G98/cycle_G98.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [StateField { index: 1, field: "feed" }, StateField { index: 2, field: "feed" }]

### 25 — cycle_repeat_distinct_RZ

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/25_cycle_repeat_distinct_RZ/cycle_repeat_distinct_RZ.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [StateField { index: 1, field: "feed" }, StateField { index: 2, field: "feed" }]

### 26 — cycle_cancel_G80

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/26_cycle_cancel_G80/cycle_cancel_G80.linuxcnc.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 2 divergences vs golden: [StateField { index: 1, field: "feed" }, StateField { index: 2, field: "feed" }]

### N3 — raw_vendor_block

- `R4` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n3_raw_vendor_block/raw_vendor_block.grbl.ngc→grbl-1.1` | ожидалось: emit отказ E_RAW_UNTRANSLATABLE | фактически: Skipped: same-dialect пара: RAW-эмиссия легальна (verbatim, CORPUS v0.5.3)
- `R4` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n3_raw_vendor_block/raw_vendor_block.linuxcnc.ngc→linuxcnc` | ожидалось: emit отказ E_RAW_UNTRANSLATABLE | фактически: Skipped: same-dialect пара: RAW-эмиссия легальна (verbatim, CORPUS v0.5.3)
