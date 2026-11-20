# REPORT-ITER23 — полный прогон корпуса через eid-harness runner

- прогон: контрольная точка итераций 2–3 (Агент B); NOTHING FIXED — сырые расхождения для классификации оркестратором (CORPUS §9)
- corpus: `v0.1-pilot` (векторов: 68)
- адаптеры: `eid-l1` = EidTextAdapter, `grbl-1.1` = GrblAdapter, `linuxcnc` = LinuxCncAdapter; R4 — translate_checked/translate_checked_golden (CORPUS v0.5.3)
- сводка runner: Pass = 346, Fail = 0, Pending = 0, Skipped = 2

## Отказы по этапам (Diagnostic.stage, interface v0.1.2)

| Этап | Fail |
|---|---:|
| Parse | 0 |
| Canonicalize | 0 |
| Verify | 0 |
| Emit | 0 |
| не-диагностический | 0 |

## Tier B — parse-or-diagnose

- файлов: 92, Ok: 12, no-adapter: 31, panics: 0
- отказы по классам: E_INVALID_INPUT × 18, E_RUNTIME_STATE × 6, E_UNSUPPORTED_SYNTAX × 25
- crash-free: true

## Счётчики по отношениям

| Отношение | Pass | Fail | Pending | Skipped |
|---|---:|---:|---:|---:|
| R1 | 48 | 0 | 0 | 0 |
| R2 | 48 | 0 | 0 | 0 |
| R3-golden | 83 | 0 | 0 | 0 |
| R3-consistency | 35 | 0 | 0 | 0 |
| R1-text | 8 | 0 | 0 | 0 |
| R4 | 8 | 0 | 0 | 2 |
| parse | 116 | 0 | 0 | 0 |

## Покрытие R3 (CORPUS v0.6)

R3 (any-to-any) доказывается ТОЛЬКО на материализованном подмножестве: **R3 на 48 из 48 позитивных** CORPUS §1.1.

Отсутствующие 0 векторов:


## Не-Pass позиции (каждый вектор, каждая непрошедшая проверка)

### N3 — raw_vendor_block

- `R4` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n3_raw_vendor_block/raw_vendor_block.grbl.ngc→grbl-1.1` | ожидалось: emit отказ E_RAW_UNTRANSLATABLE | фактически: Skipped: same-dialect пара: RAW-эмиссия легальна (verbatim, CORPUS v0.5.3)
- `R4` | файл/цель: `C:\Users\Pashtet\Desktop\computer_numerical_control\crates\eid-harness\../../corpus\tier_a/v0/n3_raw_vendor_block/raw_vendor_block.linuxcnc.ngc→linuxcnc` | ожидалось: emit отказ E_RAW_UNTRANSLATABLE | фактически: Skipped: same-dialect пара: RAW-эмиссия легальна (verbatim, CORPUS v0.5.3)


## ФОРМАЛЬНОЕ ЗАКРЫТИЕ итерации 3

- 68 векторов: 48 позитивов / 20 негативов (invalid_cases);
- R3 (any-to-any) — 48/48 (покрытие полное, отсутствующих нет);
- two-class negatives — 20/20 (Err-класс: код == expect; line-level: Ok + diagnostics);
- R1-text — 8 Pass, включая #47 (payload 24240D0A, D9-терминатор) и N3/N9;
- R4-ожидания (r4-отказы #27/#47, warning-цели #40/#41/#45, lossy-класс #41/#45) — зелёные;
- красных нет.
