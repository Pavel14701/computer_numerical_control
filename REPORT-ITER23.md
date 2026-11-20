# REPORT-ITER23 — полный прогон корпуса через eid-harness runner

- прогон: контрольная точка итераций 2–3 (Агент B); NOTHING FIXED — сырые расхождения для классификации оркестратором (CORPUS §9)
- corpus: `v0.1-pilot` (векторов: 57)
- адаптеры: `eid-l1` = EidTextAdapter, `grbl-1.1` = GrblAdapter, `linuxcnc` = LinuxCncAdapter; R4 — translate_checked/translate_checked_golden (CORPUS v0.5.3)
- сводка runner: Pass = 292, Fail = 3, Pending = 0, Skipped = 2

## Счётчики по отношениям

| Отношение | Pass | Fail | Pending | Skipped |
|---|---:|---:|---:|---:|
| R1 | 37 | 0 | 0 | 0 |
| R2 | 37 | 0 | 0 | 0 |
| R3-golden | 69 | 1 | 0 | 0 |
| R3-consistency | 33 | 0 | 0 | 0 |
| R1-text | 7 | 0 | 0 | 0 |
| R4 | 8 | 0 | 0 | 2 |
| parse | 101 | 2 | 0 | 0 |

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

### 47 — raw_crlf_fidelity

- `R3-golden` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/47_raw_crlf_fidelity/raw_crlf_fidelity.grbl.ngc` | ожидалось: parse(file) ≡ golden AST (semantic_diff = 0) | фактически: Fail: 3 divergences vs golden: [OpKind { index: 0 }, OpKind { index: 1 }, OpKind { index: 2 }]

### N3 — raw_vendor_block

- `R4` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n3_raw_vendor_block/raw_vendor_block.grbl.ngc→grbl-1.1` | ожидалось: emit отказ E_RAW_UNTRANSLATABLE | фактически: Skipped: same-dialect пара: RAW-эмиссия легальна (verbatim, CORPUS v0.5.3)
- `R4` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n3_raw_vendor_block/raw_vendor_block.linuxcnc.ngc→linuxcnc` | ожидалось: emit отказ E_RAW_UNTRANSLATABLE | фактически: Skipped: same-dialect пара: RAW-эмиссия легальна (verbatim, CORPUS v0.5.3)

### N4 — peck_L_over_limit

- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n4_peck_L_over_limit/peck_L_over_limit.linuxcnc.ngc` | ожидалось: отказ E_LIMIT_EXCEEDED | фактически: Fail: expected refusal E_LIMIT_EXCEEDED, got E_UNSUPPORTED_SEMANTICS

### N6 — naked_coords_after_G80

- `parse` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n6_naked_coords_after_G80/naked_coords_after_G80.linuxcnc.ngc` | ожидалось: отказ E_INVALID_INPUT | фактически: Fail: expected refusal E_INVALID_INPUT, got E_UNSUPPORTED_SEMANTICS
