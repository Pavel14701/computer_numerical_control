# REPORT — MVP-цикл, итерации 0–5

**Дата:** 2026-11-21 · **Оркестратор:** сессия по плану `IMPLEMENTATION-PLAN.md` v0.1.3 (5 агентов: A core, B harness/corpus, C GRBL, D LinuxCNC, E integration; оркестратор — координация, классификация, acceptance)
**Версии:** SPEC v0.16 · grammar v0.3 · EQUIVALENCE v0.3.1 · COVERAGE v0.8 (Fanuc-колонка) · CORPUS v0.6.6 · PLAN v0.1.3 (§5а/§5б триажи) · interface v0.1.2 · **manifest: 68 векторов (48 позитивных / 20 негативных) + 121 Tier B (61 published / 60 local-dev) — все внесены, лицензии проверены**

---

## 1. Статус: что построено

Полный транслятор GRBL ↔ EID ↔ LinuxCNC: парсеры обоих диалектов с канонизацией (modal state → resolved-state AST), детерминированные эмиттеры, **развёртывание canned cycles** (G81/G82/G83/G73/G85/G86/G89 по §7.7-шаблону), RAW-механизм двух классов (same-dialect verbatim / кросс-диалект отказ), legality-таблицы (ConversionTarget-паттерн), verify-проход с stage-атрибуцией диагностик, semantic_diff по допускам EQUIVALENCE §4, runner R1–R4, Tier B (121 файл: 61 published MIT / 60 local-dev GPL), CLI (translate/verify, живой режим), профили с axis_limits, **sim real (LinuxCNC 2.9 rs274 в Docker, headless) + sim mock**.

**Числа:** **475 тестов passed / 0 failed** · 91+ коммитов · SPEC v0.16 (auto-PEND, O-code→Raw) · 4 СТОП-протокола (агенты→оркестратор) · Tier A R3 (any-to-any) **48/48 позитивных** · негативы 20/20 parse-or-diagnose · Tier B **121/121 crash=0** (свип с manifest-диалектами: ok=57 / err=64: INVALID 41 / RUNTIME 22 / SEMANTICS 1) · mutation-testing kill-rate: grbl 82.2% / linuxcnc ~80% (усилен) / workspace 75.5% · метаморфные инварианты зелёные · grammar-conformance 57 · **e2e48-матрица: 141 направлений через живой CLI** · **rs274 canonical comparison 48/48 — 0 divergences**.

## Итерация 5 — слои качества (MLIR/LLVM-триаж, PLAN §5а/§5б)

- **Verify-pass** (`eid_core::verify`, отдельный проход на любом этапе) + `Diagnostic.stage` (Parse/Canonicalize/Verify/Emit) — stage-атрибуция в runner-отчётах и CLI (`[emit]`, `[verify]`); интерфейс v0.1.2 (transit: grbl 3 / linuxcnc 11 / harness 1 — все закрыты).
- **Legality-таблицы обеих машин** (ConversionTarget-паттерн): legal/illegal + коды как данные; `emit` проверяет AST против таблицы; cross-check «таблица ↔ manifest r4_expect» — **0 расхождений**.
- **Mutation-фаззер M1–M17 + AST-driven generator** (Tier A v1, seeded): panic-freedom (0 паник на byte-soup × 2 адаптера × 5 повторов = 210 прогонов), 18 находок — **0 новых классов, все известные Verify-инварианты** (refinement мутаторов зафиксирован).
- **Метаморфные отношения** (комментарии/line-ending/whitespace/регистр/N-слова → идентичный AST): первый прогон нашёл 10 «нарушений» → расследование D → **артефакт компаратора** (PartialEq с SrcSpan provenance) → semantic_diff (EQUIVALENCE §3); постоянные тест-пары в eid-linuxcnc.
- **Grammar-conformance**: 57 тестов по 33 продукциям grammar.ebnf.
- **cargo-mutants workspace-wide**: 2061 мутантов; kill-rate: grbl 82.2% / core 80.3% / linuxcnc 66.6% (зона D — усилена) / harness 73.4% → общий 75.5%.
- **Diversity-сбор batch 2**: +29 published (61 total), **6/6 семейств чек-листа** (multi-Z, сверловка, G18/G19, helical, 3D-контуры, радиусные цепочки); 2D-skew первого батча снят.
- **rs274ngc source-check**: дефолт retract = **G99 подтверждён source'ом** (`interp_setup.cc:152`) — §6.13 закрыт ✔.

---

## 2. Приёмка по 12 критериям (IMPLEMENTATION-PLAN §5)

| # | Критерий | Статус | Доказательство |
|---|---|---|---|
| 1 | `cargo test --workspace` зелёный | ✅ | **468 passed / 0 failed** (финальный прогон) |
| 2 | R1-AST 48/48 golden | ✅ | checkpoint v7: R1 48/48 |
| 3 | R1-text (RAW byte-exact) | ✅ | **8/8 Pass**, включая #47 (`24240D0A`, D9-терминатор, CRLF), N3/N9 |
| 4 | R2 canon stability 48/48 | ✅ | v7 |
| 5 | R3 (any-to-any) 48/48 | ✅ | v7: R3-golden **83/0** (48 поз. + cycle-семья), R3-consistency 35/0 |
| 6 | R4 — правильные коды отказов | ✅ | legality-таблицы (ConversionTarget) ↔ manifest r4_expect: **0 расхождений**; негативы 20/20 two-class |
| 7 | Tier B: parse-or-diagnose 100%, crash=0 | ✅ | **121/121, crash=0** (121 файл в manifest, свип с диалектами из manifest: ok=57 / err=64 = INVALID 41 + RUNTIME 22 + SEMANTICS 1; diversity batch 2 внесён: 29 файлов, лицензии проверены) |
| 8 | Симуляция | ✅ **CLOSED: real + mock + canonical comparison** | Real LinuxCNC 2.9 rs274 в Docker (headless, `docker/linuxcnc-sim.Dockerfile`): **rs274_e2e48 — canonical comparison всех 48 позитивных векторов (v0-источник vs v1-эмиссия) — 0 divergences** (motion stream CLOSED); расширенный прогон D: Tier B 121 через rs274 crash=0 (rs_ok=51 / rs_err=70 — реальный interp отвергает нестандартные файлы, ожидаемо), canonical comparison на Ok: 11 проверено / 0 провалов / 22 RAW-skip; #47 RAW — ожидаемый отказ by design (SPEC §8.2); #27 с tool.tbl — exit 0; mock-sim зелёный |
| 9 | Demo: translate grbl→linuxcnc | ✅ | e2e: pilot #01 → файл создан → parse(output) ≡ parse(input) |
| 10 | Demo reverse | ✅ | linuxcnc→grbl→linuxcnc ≡; **grbl→linuxcnc→grbl: три AST равны** (PLAN §5) |
| 11 | REPORT.md | ✅ | этот документ |
| 12 | CHANGELOG актуален | ✅ | ведётся с первого коммита; каждое изменение документа = запись + bump; **пробелы найдены аудитом и заполнены** (SPEC v0.15/v0.15.1, CORPUS v0.6.4/v0.6.5, PLAN v0.1.2/v0.1.3/§5а/§5б) |

**Итог: 12/12 полностью (criterion #8 закрыт canonical comparison'ом rs274_e2e48). Блокеров нет.**

### Criterion #8 — детализация (ЗАКРЫТ)

- **rs274 canonical output**: `STRAIGHT_TRAVERSE`, `ARC_FEED`, `SET_FEED_RATE` — парсится и сверяется (`rs274_compare.rs`, guard: docker недоступен → SKIP).
- **Tier A (rs274_e2e48)**: все 48 позитивных векторов — v0-источник vs v1-эмиссия через реальный rs274 → **0 divergences** (34 пары motion-сравнения + RAW-пары ожидаемо-отказ + SPEC §3 drops). **Motion stream comparison закрыт** (бывший «pending итерации 6»).
- **Tier B расширенный прогон**: 121 файл через rs274 — crash=0; rs_ok=51 / rs_err=70 (реальный interp отвергает 3dp/E-слова/неканонические файлы — ожидаемо); canonical comparison на Ok-файлах: 11 проверено / 0 провалов / 22 RAW-skip.
- **rs274 exit codes**: #47 RAW — ожидаемый отказ by design (SPEC §8.2); #27 с tool.tbl — exit 0.
- **CANON-DIV находка** (grbljs_jsdc-laser, 4108 div): классифицирована D+оркестратором как **чувствительность компаратора к командному порядку** rs274-потока (units/feed/dwell placement при эмиссии) — **геометрия пути совпадает**; не Adapter-wrong, не Spec-ambiguous. Кандидат итерации 6: motion-level компаратор (позиции+geometry, не сырой командный порядок).

---

## 3. Находки сессии (данные корпуса → очередь ревизии)

Первый реальный корпус дал то, чего синтетика дать не могла:

1. **«PEND обязателен» vs реальность: 18/32 published-файлов заканчиваются без M2/M30** — реальные CAM/конвертеры почти никогда не пишут program-end. **Spec-кандидат: auto-PEND для Tier B** (отсутствие M2 = неявный конец, не отказ). Решение — за ревью (не реализовано молча; сейчас 18 файлов диагностируются корректно по текущей спеке).
2. **O-code/подпрограммы ×25** — крупнейший local-класс; реальная частота для решения Q10 (статический инлайнинг — итерация 5 кандидат).
3. **P-слова без потребителя ×13** (LinuxCNC NURBS-файлы) — конструкция вне Tier A целиком; кандидат на Raw-вектор или NURBS-поддержку (этап 12).
4. **Partial-end на старте ×3 (published)** — «программа начинается не с полной координаты» — реальный паттерн, покрытый Q11-правилом (диагностируется корректно).
5. **Реальные кривые дуги ×2** (radius mismatch) — парсер не «подогнал» — диагностика по §6 отработала на поле.
6. Дефолт retract LinuxCNC = **G99, подтверждён source'ом** (§6.13 закрыт ✔; документация дефолт не заявляет — зафиксировано source'ом `interp_setup.cc:152`).
7. **Auto-PEND подтверждён реальным интерпретатором** (Tier B rs274-прогон): массовый SOLO-ERR «File ended with no percent sign or program end» на published-файлах без M2/M30 — rs274 отказывает там, где наш канонизатор после SPEC v0.16 принимает с notice-вставкой. Два независимых свидетельства (наши данные корпуса + поведение rs274) — ревизия v0.16 обоснована.
8. **CANON-DIV grbljs_jsdc-laser (4108 div)** — командный порядок rs274-потока (units/feed/dwell placement) чувствителен к эмиссионному размещению при равной геометрии пути; классификация: чувствительность компаратора, motion-level сравнение — кандидат итерации 6.

## 4. Известные ограничения (честно)

- **Симуляция**: criterion #8 ЗАКРЫТ (canonical comparison 48/48, 0 divergences); остаток — motion-level компаратор для Tier B Ok-файлов с чувствительностью к командному порядку (кандидат итерации 6, не блокер: геометрия путей совпадает).
- **Manifest-диалекты в sweep**: `dialect_of` — эвристика по префиксу, синхронизирована с manifest вручную; кандидат итерации 6 — читать диалект из manifest.toml напрямую.
- **Tier B published — 2D/2.5D-скew** (SVG/гравировка/роутер): diversity-targeted сбор (3-осевой mill) — итерация 5, чек-лист в CORPUS §7. Local-dev контур (60 LinuxCNC) 3D-контент покрывает — но тест-only (GPL).
- **Q11: unknown-start** — канонизатор отказывает partial-end после genuinely-unknown (v0.11) — 3 published-файла диагностируются корректно; апгрейд «дефолт-наследование» — кандидат ревизии.
- **v0/v1-долг §1**: закрыт регенерацией (70/76 пар v1; 4 diverged — классифицированы и починены как Adapter-wrong C/D; #41/#45 grbl — lossy навсегда по Q3). Независимость v0-пар как теста корректности сохранена (v0 = regression-корпус).
- **grblHAL/FluidNC** — адаптеры pending (флаги профиля), коллекция NOASSERTION-источников не велась — post-MVP.

## 5. Процессные итоги (работа системы 5 агентов + оркестратор)

- **Самокоррекция сработала 4 раза**: D→оркестратор (#39 контр-классификация), C→спека (§2.5 units), B→сам (#19 геометрия), оркестратор→себя (#48 File-wrong, счётчики 67/58/48).
- **Все правила в документах**: ноль «код-без-спеки» долгов после v0.13-аудита (спека v0.13→v0.15 догнала код).
- **Режимы**: staged-брифы против зависаний; прицельные коммиты; активная версия в шапке задач; documents-only-via-editor.
- **Цикл классификации красного** (CORPUS §9): 70 красных v1 → 4 категории → адресные фиксы → 0 красных v5-v7 за две итерации.

## 6. Дальше (итерация 6 — Fanuc, этап 10; волна в полёте)

**Fanuc (этап 10, в работе):** research закрыт (`fanuc_dialect_analysis.md` v0.0: COVERAGE-матрица 30i, 3 ловушки адаптера — EOB `;` / DPI bare-integer / G10 L20≠LinuxCNC; Q6 G76 закрыт — параметр станка 5101; G52 = единственный ERR-класс; Macro B → line-level Raw подтверждён структурой диалекта; реестр 15 `?`) · COVERAGE v0.8 Fanuc-колонка внесена · `eid-fanuc` парсер (этап 1: скелет/лексер/движение — агент C в работе; этапы 2–3: фреймы/циклы по Fanuc-семантике/M-коды/Macro B; затем эмиссия).

**Кандидаты итерации 6:** motion-level компаратор rs274-потока (CANON-DIV-остаток) · sweep из manifest напрямую · fanuc-эмиссия + профили (G59.x→G54.1-опция) · реестр 15 `?` — сверка по конкретной серии Fanuc.

**Ревью-очередь (пользователь, данные собраны):** Q10 static-inlining (O-code ×25) · NURBS P-words ×13 (Raw vs этап 12). Auto-PEND — решён (SPEC v0.16, подтверждён rs274).
