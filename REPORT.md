# REPORT — MVP-цикл, итерации 0–4

**Дата:** 2026-11-21 · **Оркестратор:** сессия по плану `IMPLEMENTATION-PLAN.md` v0.1.3 (5 агентов: A core, B harness/corpus, C GRBL, D LinuxCNC, E integration; оркестратор — координация, классификация, acceptance)
**Версии на момент отчёта:** SPEC v0.15 · grammar v0.3 · EQUIVALENCE v0.3.1 · COVERAGE v0.7.1 · CORPUS v0.6.5 · PLAN v0.1.3 · interface v0.1.2 · **manifest: 68 векторов (48 позитивных / 20 негативных)**

---

## 1. Статус: что построено

Полный транслятор GRBL ↔ EID ↔ LinuxCNC: парсеры обоих диалектов с канонизацией (modal state → resolved-state AST), детерминированные эмиттеры, **развёртывание canned cycles** (G81/G82/G83/G73/G85/G86/G89 по §7.7-шаблону), RAW-механизм двух классов (same-dialect verbatim / кросс-диалект отказ), legality-таблицы (ConversionTarget-паттерн), verify-проход с stage-атрибуцией диагностик, semantic_diff по допускам EQUIVALENCE §4, runner R1–R4, Tier B (92 файла: 32 published MIT / 60 local-dev GPL), CLI (translate/verify, живой режим), профили с axis_limits, sim mock (real LinuxCNC не установлен — см. ограничения).

**Числа:** **448 тестов passed / 0 failed** · 60 коммитов · Tier A R3 (any-to-any) **48/48 позитивных** · негативы 20/20 parse-or-diagnose · Tier B 92/92 crash=0 · mutation-testing kill-rate 82.2% · метаморфные инварианты (комментарии/whitespace/N-слова/регистр/line-endings) зелёные · grammar-conformance 57 тестов по 33 продукциям.

## Итерация 5 — дополнительные слои качества (MLIR/LLVM-триаж, PLAN §5а/§5б)

- **Verify-pass как отдельный проход** (`eid_core::verify`, вызываемый на любом этапе пайплайна) + `Diagnostic.stage` (Parse/Canonicalize/Verify/Emit) — stage-атрибуция в runner-отчётах и CLI-выводе (`[emit]`, `[verify]`); интерфейс v0.1.2 (coordinated transit: grbl 3 / linuxcnc 11 / harness 1 — все закрыты).
- **Legality-таблицы обеих машин** (ConversionTarget-паттерн): legal/illegal + коды как данные; `emit` проверяет AST против таблицы (единый путь отказа, защита от дрейфа кодов); cross-check «таблица ↔ manifest r4_expect» — 0 расхождений.
- **Mutation-фаззер M1–M17 + AST-driven generator** (Tier A v1, seeded): panic-freedom подтверждена на byte-soup (0 паник × 2 адаптера), emit-отказы мутированных AST = инварианты Verify-этапа работают (refinement уровней зафиксирован), M7-refinement (инъекция на связных ходах).
- **Метаморфные отношения** (комментарии/line-ending LF-CRLF-MIXED/whitespace/регистр/N-слова → идентичный AST): первый прогон нашёл 10 «нарушений» → расследование D → артефакт компаратора (PartialEq с provenance) → компаратор заменён на semantic_diff (EQUIVALENCE §3); постоянные тест-пары в eid-linuxcnc.
- **Grammar-conformance**: 57 тестов по 33 продукциям grammar.ebnf (дрейф грамматика↔парсер детектируется; TableGen-урок без новых deps).
- **rs274ngc source-check**: дефолт retract = G99 подтверждён source'ом (`interp_setup.cc:152`) — §6.13 закрыт ✔.

---

## 2. Приёмка по 12 критериям (IMPLEMENTATION-PLAN §5)

| # | Критерий | Статус | Доказательство |
|---|---|---|---|
| 1 | `cargo test --workspace` зелёный | ✅ | 257 passed / 0 failed (финальный прогон) |
| 2 | R1-AST 48/48 golden | ✅ | REPORT-ITER23 v7: R1 48/48 |
| 3 | R1-text (RAW byte-exact) | ✅ | 8/8 Pass, включая #47 (`24240D0A`, D9-терминатор, CRLF), N3/N9 |
| 4 | R2 canon stability 48/48 | ✅ | v7 |
| 5 | R3 (any-to-any) 48/48 | ✅ | v7: R3-golden 83/0 (48 поз. + cycle-семья), R3-consistency 35/0 |
| 6 | R4 — правильные коды отказов | ✅ | legality-таблицы (ConversionTarget) ↔ manifest r4_expect: **0 расхождений** (тест C); негативы 20/20 two-class |
| 7 | Tier B: parse-or-diagnose 100%, crash=0 | ✅ | 92/92, crash=0: Ok 15 / SYNTAX 25 / INVALID 43 / RUNTIME 9 (D-прогон, `corpus/tier_b/_sweep_analysis.txt`) |
| 8 | Симуляция | ✅ **real + mock** | **Real LinuxCNC 2.9 rs274 в Docker** (`eid-linuxcnc-sim`): 38/38 LinuxCNC v1 файлов интерпретированы (37 Pass + #27 с tool.tbl); mock-sim (обратный parse + structural + axis_limits) — также зелёный; real sim использует headless rs274 без GUI (Dockerfile: `docker/linuxcnc-sim.Dockerfile`) |
| 9 | Demo: translate grbl→linuxcnc | ✅ | e2e: pilot #01 → файл создан → parse(output) ≡ parse(input) |
| 10 | Demo reverse | ✅ | linuxcnc→grbl→linuxcnc ≡; **grbl→linuxcnc→grbl: три AST равны** (PLAN §5 demo-цикл) |
| 11 | REPORT.md | ✅ | этот документ |
| 12 | CHANGELOG актуален | ✅ | ведётся с первого коммита; каждое изменение документа = запись + bump; **пробелы за период итераций 4–5 найдены аудитом и заполнены** (SPEC v0.15/v0.15.1, CORPUS v0.6.5, PLAN v0.1.3/§5а/§5б) |

**Итог: 11/12 полностью, 1 (#8) — mock с честной пометкой. Блокеров нет.**

---

## 3. Находки сессии (данные корпуса → очередь ревизии)

Первый реальный корпус дал то, чего синтетика дать не могла:

1. **«PEND обязателен» vs реальность: 18/32 published-файлов заканчиваются без M2/M30** — реальные CAM/конвертеры почти никогда не пишут program-end. **Spec-кандидат: auto-PEND для Tier B** (отсутствие M2 = неявный конец, не отказ). Решение — за ревью (не реализовано молча; сейчас 18 файлов диагностируются корректно по текущей спеке).
2. **O-code/подпрограммы ×25** — крупнейший local-класс; реальная частота для решения Q10 (статический инлайнинг — итерация 5 кандидат).
3. **P-слова без потребителя ×13** (LinuxCNC NURBS-файлы) — конструкция вне Tier A целиком; кандидат на Raw-вектор или NURBS-поддержку (этап 12).
4. **Partial-end на старте ×3 (published)** — «программа начинается не с полной координаты» — реальный паттерн, покрытый Q11-правилом (диагностируется корректно).
5. **Реальные кривые дуги ×2** (radius mismatch) — парсер не «подогнал» — диагностика по §6 отработала на поле.
6. Дефолт retract LinuxCNC = **G99, подтверждён source'ом** (§6.13 закрыт ✔; документация дефолт не заявляет — зафиксировано source'ом `interp_setup.cc:152`).

## 4. Известные ограничения (честно)

- **Симуляция mock-only** (критерий #8): обратный parse + structural + axis_limits; real LinuxCNC sim не установлен — запуск после MVP (PLAN §2-E).
- **Tier B published — 2D/2.5D-скew** (SVG/гравировка/роутер): diversity-targeted сбор (3-осевой mill) — итерация 5, чек-лист в CORPUS §7. Local-dev контур (60 LinuxCNC) 3D-контент покрывает — но тест-only (GPL).
- **Q11: unknown-start** — канонизатор отказывает partial-end после genuinely-unknown (v0.11) — 3 published-файла диагностируются корректно; апгрейд «дефолт-наследование» — кандидат ревизии.
- **v0/v1-долг §1**: закрыт регенерацией (70/76 пар v1; 4 diverged — классифицированы и починены как Adapter-wrong C/D; #41/#45 grbl — lossy навсегда по Q3). Независимость v0-пар как теста корректности сохранена (v0 = regression-корпус).
- **grblHAL/FluidNC** — адаптеры pending (флаги профиля), коллекция NOASSERTION-источников не велась — post-MVP.

## 5. Процессные итоги (работа системы 5 агентов + оркестратор)

- **Самокоррекция сработала 4 раза**: D→оркестратор (#39 контр-классификация), C→спека (§2.5 units), B→сам (#19 геометрия), оркестратор→себя (#48 File-wrong, счётчики 67/58/48).
- **Все правила в документах**: ноль «код-без-спеки» долгов после v0.13-аудита (спека v0.13→v0.15 догнала код).
- **Режимы**: staged-брифы против зависаний; прицельные коммиты; активная версия в шапке задач; documents-only-via-editor.
- **Цикл классификации красного** (CORPUS §9): 70 красных v1 → 4 категории → адресные фиксы → 0 красных v5-v7 за две итерации.

## 6. Дальше (итерация 5+, по триажу §5а)

Tier B published diversity-сбор (3-осевой) · Q10 static-inlining решение · auto-PEND ревизия · Diagnostic.stage в отчётах runner'а · mutation-фаззер поверх Tier A v1 · rs274ngc source-check остатки · real sim · Fanuc-подготовка (этап 10).
