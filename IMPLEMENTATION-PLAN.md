# IMPLEMENTATION-PLAN — 5 субагентов + оркестратор

**Версия:** v0.1.3 (R-1 миграция выполнена E+B; правило активной версии + правило редактора; retract_clearance ruling: SPEC-константа, профиль не переопределяет; привязан к SPEC v0.15, COVERAGE v0.7, EQUIVALENCE v0.3.1, CORPUS v0.6.3, grammar v0.2)
**Правило:** все формулировки о поведении диалектов сверяются с COVERAGE (матрица + реестр `?`); формулировки из ранних ревью без source-чека в план не возвращать.

---

## 0. Scope и критерий «полная реализация»

**MVP = этапы 0–8 исходной дорожной карты:**

- EID/L1 работает как хаб;
- GRBL ↔ EID ↔ LinuxCNC (any-to-any на двух диалектах);
- machine profile как декларативные данные;
- верификация через симулятор (или mock с пометкой);
- demo: реальный GRBL-файл → EID → LinuxCNC-файл → симуляция → тот же motion stream, и обратно.

**Вне scope MVP (этапы 9–12):** ИИ-харнесс, Fanuc-семейство, 5 осей, макросы/probing — future, не acceptance.

**Оценка:** 6 итераций по 5–7 агентских дней; календарь 2–3× — нормально.

---

## 1. Интерфейсы — ЗАМОРОЖЕНЫ (v0.1-interface)

Заморожены как Rust-стабы в `crates/` (см. итерацию 0). Сигнатуры — контракт; изменение — только PR к оркестратору + rebase всех агентов + bump `v0.2-interface`.

| Интерфейс | Расположение | Потребители |
|---|---|---|
| `eid_core::ast` — Program, Op, State, SrcSpan, FramePreset, Point3, Num, StartPoint | `crates/eid-core/src/ast.rs` | B, C, D, E |
| `eid_core::diag` — Diagnostic, ErrorCode (7 кодов SPEC §8), SrcSpan | `crates/eid-core/src/diag.rs` | B, C, D, E |
| `eid_core::adapter::Adapter` — parse/emit/dialect_name | `crates/eid-core/src/adapter.rs` | C, D, E |
| `eid_harness::diff::semantic_diff` + DiffConfig (допуски EQUIVALENCE §4) | `crates/eid-harness/src/diff.rs` | A, C, D, E |
| `eid_harness::manifest` — Manifest, Vector, r4_expect, notice | `crates/eid-harness/src/manifest.rs` | A, C, D, E |
| `eid_integration::profile::MachineProfile` | `crates/eid-integration/src/profile.rs` | C, D |

**Repo layout** — существующий corpus layout НЕ переизобретается (manifest — контракт):

```
corpus/                     # уже существует: manifest.yaml + tier_a/v0/<vector>/{golden.eid, *.grbl.ngc, *.linuxcnc.ngc}
crates/
  eid-core/         (A)     # + parser.rs, emitter.rs — реализация, не интерфейс
  eid-grbl/         (C)
  eid-linuxcnc/     (D)
  eid-harness/      (B)     # + runner.rs — реализация
  eid-integration/  (E)     # + sim.rs — реализация
  eid-cli/          (E)
tools/ebnf_check.sh         # reference-валидация grammar (bottlecaps.de/rr/ui)
.github/workflows/ci.yml
```

---

## 2. Субагенты

### Агент A — EID Core

**Ответственность:** AST + валидация, диагностика, парсер и эмиттер текстового формата EID, round-trip identity.

**Артефакты:** `ast.rs` (типы + валидация §3), `diag.rs`, `adapter.rs` (trait + EidTextAdapter), `parser.rs`, `emitter.rs`, `tests/roundtrip.rs`.

**Ключевые правила реализации (источники — SPEC v0.9):**
- unknown-старт: **точки разрыва (a)–(d) + обязанность связывания** (§2, принцип 2; §3) — НЕ «unknown только у первого опа»;
- program_end ровно один раз, последний (§3);
- zero-length motion → drop+notice (§3); linear/arc в итоговом AST требуют feed;
- emitter детерминирован: один оп = одна строка, 4dp, trailing zeros, порядок state-полей fixed (grammar D1–D10);
- числа: verbatim-строка + f64-зеркало (§10.1), сравнение на f64 (EQUIVALENCE §4).

**DoD:** R1-AST зелёный на **всех 47** golden AST (включая unknown-start из Q11); двойной emit байт-в-байт; все 7 кодов диагностики реализованы; интерфейсы не изменены.

**Риск:** Q11 — самый частый источник багов; правило — точки разрыва, не «первый оп».

### Агент B — Harness & Corpus

**Ответственность:** golden AST (материализация до 47 векторов), manifest, semantic_diff, runner, reference EBNF validation, Tier B collection.

**Артефакты:** `diff.rs`, `runner.rs`, `manifest.rs`, материализация векторов в существующем corpus layout, `tools/ebnf_check.sh`, `.github/workflows/ci.yml`, `tools/changelog_check.sh` (критерий #12).

**Митигация v0/v1-долга:** B пишет недостающие golden AST **после** A выпустит парсер (AST проверен round-trip'ом A), диалектные файлы — независимо (v0-долг закрывается регенерацией на итерации 3–4).

**DoD:** 47 golden валидны через reference EBNF-парсер; semantic_diff = 0 на идентичных, ловит edge cases (`golden_semicircle_R`, `preset_G54_then_switch`); негативы N1–N19 в manifest с ожидаемыми кодами; Tier B 50–100 файлов с provenance; R1-text per-file по manifest.

### Агент C — GRBL canonicalizer + emitter

**Ответственность:** полный цикл GRBL ↔ EID.

**quirks.rs — ИСПРАВЛЕНО (источники: COVERAGE §5/§6.11, SPEC §7):**
- **G17/G18/G19: I/J/K literal X/Y/Z — ремапа НЕТ** (gcode.c «not remapped per plane»). Эмиссия — **in-plane пары**: G17 → I+J, G18 → I+K, G19 → J+K; unused offset можно опускать (GRBL: cleared, не FAIL);
- R-форма → IJK при канонизации, всегда (EQUIVALENCE §4 политика);
- canned cycles → развёртывание §7 SPEC; G86 — рестарт шпинделя обязателен;
- feed переживает G0 (§4); zero-length → drop+notice;
- **tool_change → `E_UNSUPPORTED_SYNTAX`** (M6 вне словаря GRBL — правило B2);
- **G10 L20 → узел wcs_set_here, отказа НЕТ** (в словаре GRBL 1.1, README Non-Modal);
- **G61.1/path_mode → warning с записью потери** (Q3), не отказ;
- G43 H → `E_REQUIRES_MACHINE_TABLE`; G92 mid-program → отказ; одна система единиц на программу;
- пороги `E_LIMIT_EXCEEDED` — из COVERAGE §1.2, переопределяются профилем (агент E).

**DoD:** R1-AST зелёный на GRBL-векторах, где источник выразим (cycle-векторы #17–#26 — без GRBL-источника: parse невозможен, эмиссия развёрнутых ходов — да); R2 зелёный; R4 — **правильные коды** по `r4_expect`; GRBL-сторона пилота: #1, #2, #3, #11, N1-негатив (**#27 и #46 — без GRBL-источника**: #27 = R4-отказ, #46 отложен до §6.9).

**Риск:** grblHAL/FluidNC — семейство, флаги профиля (E), не core. grblHAL-колонки — `?` до §6.12 source-чека.

### Агент D — LinuxCNC canonicalizer + emitter

**Ответственность:** полный цикл LinuxCNC ↔ EID.

**quirks.rs:** G90.1/G91.1 → ABSORB; O-code/M98/M99 → `E_UNSUPPORTED_SYNTAX`; `#`-параметры/system vars → `E_RUNTIME_STATE`, if/while → `E_UNSUPPORTED_SYNTAX` (SPEC §1.1 v0.3); G10 L20 vs G10 L2 — разные узлы; G92 mid-program → отказ; G93 → разрешение в G94-feed на уровне опа (§2 EQUIVALENCE); G5.x — scope decision RAW; M62–M73 → `E_UNSUPPORTED_SEMANTICS`; G76 — токарная резьба, отказ (§1.2).

**DoD:** симметрично C; Tier B LinuxCNC — parse-or-diagnose, ноль crash.

### Агент E — Profile, CLI, Simulation, Cross-dialect

**Артефакты:** `profile.rs` (MachineProfile: диалект, флаги семейства, лимиты §1.2, emit_r_arcs-флаг с notice — EQUIVALENCE §4), `profiles/grbl_default.yaml`, `profiles/linuxcnc_default.yaml`, `eid-cli` (translate/verify), `sim.rs` (LinuxCNC sim / CAMotics L3), `tests/crossdialect.rs` (R3), `tests/demo.rs`.

**DoD:** CLI end-to-end; R3 на всём Tier A 100%; sim принимает выход (или mock с пометкой); demo по §5; REPORT.md.

**Риск:** sim требует установки/контейнера → mock сначала (эмитированный G-код парсится обратно как LinuxCNC), реальный sim — итерация 4. Не блокировать MVP на sim.

---

## 3. Оркестратор

- Документы применены (v0.9/v0.2/v0.5/v0.4/grammar v0.1) — НЕ работа итерации 0.
- **Итерация 0:** workspace + стабы + CI + freeze commit + **source-чеки §6.9/§6.12**.
- **Source-чек = чек + возможная ревизия COVERAGE** (добавление ревью): если GRBL не поддерживает R+off-plane → §6.9 закрывается решением «#46-GRBL → E_UNSUPPORTED_SEMANTICS» через ревизию COVERAGE + CHANGELOG; владелец — оркестратор, срок — до итерации 4 (когда C нужен #46-GRBL).
- **Активная версия в шапке каждой задачи** (v0.1.2, ревью итерации 3): каждый бриф агенту начинается со строки «активная SPEC = vX.Y, grammar = vA.B, EQUIVALENCE = vC.D» — снимает класс ошибок «работал по устаревшей версии» (случай: ревью итерации 3 писалось по памяти о v0.8-v0.9 против применённых v0.10-v0.13).
- **Документы — только через редактор с явным diff, никогда через `sed`/потоковые правки** (v0.1.2, ревью итерации 3: sed по кириллице/CJK сбросил read-state и дал скрытый риск тихой потери правки). Инцидент: правка «inch-файл» прошла через sed — единственный случай, пойман сразу; правило фиксирует запрет.
- Мержит PR, разрешает конфликты; финальная приёмка по §5.
- Не пишет рабочий код (только стабы); архитектурные вопросы эскалируются.

---

## 4. Итерации

### Итерация 0 — оркестратор (выполнена 2026-11-20)

1. ~~Применить документы~~ — применены ранее.
2. Repo layout + Cargo workspace + CI skeleton + `tools/`.
3. **Интерфейсы v0.1 frozen** — стабы с сигнатурами; commit `interface-freeze-v0.1` + тег.
4. N1 перемещён в `tier_a/v0/` (негативы — Tier A по CORPUS §1.2).
5. Source-чеки §6.9/§6.12 — запущены (оркестратор).

### Итерация 1 — kickoff (параллельно)

| Агент | Работа |
|---|---|
| A | **На полную:** ast-валидация (§3), EID parser + emitter по grammar v0.1, EidTextAdapter, round-trip на 8 пилотных golden, юнит-тесты диагностики |
| B | rr-ui валидация 8 golden по grammar v0.1; runner skeleton; **негативы N2–N19** материализация + manifest; `changelog_check.sh` |
| C | GRBL lexer skeleton; quirks-план по ИСПРАВЛЕННОМУ списку §2-C |
| D | LinuxCNC lexer skeleton; O-code rejection план |
| E | MachineProfile schema (YAML), CLI skeleton (аргументы), sim mock план |

**Выход:** A закрыт (R1 на пилоте), B имеет runner + негативы, C/D/E на стабах.

### Итерация 2 — core+parsers

| A | Багфиксы от B; краевые случаи (Q11, zero-length, notice) |
|---|---|
| B | Golden AST #4–#16 (по образцу пилота, после парсера A); semantic_diff v0 |
| C | GRBL parser простых программ, canonicalizer v0 |
| D | LinuxCNC parser простых программ, canonicalizer v0 |
| E | Profile YAML; CLI аргументы |

**Выход:** R1-AST зелёный на пилоте (A+B), R1 на простых диалектных файлах.

### Итерация 3 — emitters

| A | Багфиксы; edge cases |
|---|---|
| B | Все 47 golden; reference EBNF check полный; Tier B collection start |
| C | GRBL emitter v0; R1 на GRBL-корпусе |
| D | LinuxCNC emitter v0; R1 на LinuxCNC-корпусе |
| E | CLI translate end-to-end (со стабами C/D до их готовности) |

**Выход:** первый end-to-end GRBL→EID→GRBL и LinuxCNC→EID→LinuxCNC. **Регенерация v0-пар начинается здесь** (закрытие долга §1 CORPUS).

### Итерация 4 — full coverage

| A | Full round-trip stable на всём Tier A |
|---|---|
| B | Tier B complete (50–100 файлов), R1-AST/R1-text per manifest |
| C | GRBL R4 expectations (#27 SYNTAX, #40/#41/#45 warning) — правильные коды |
| D | LinuxCNC R4; полный прогон Tier B |
| E | Реальный sim; CLI verify |

**Выход:** R1, R2, R4 зелёные на всём Tier A; v0-пары регенерированы (долг закрыт).

### Итерация 5 — cross-dialect

| A | Багфиксы от R3 |
|---|---|
| B | Full test suite, все R1–R4 |
| C | Full GRBL, edge cases (#36 preset_G54_then_switch, #46 — после §6.9-решения) |
| D | Full LinuxCNC, edge cases |
| E | R3 на всём Tier A, integrated demo |

**Выход:** R3 зелёный на всём Tier A — «any-to-any доказан».

### Итерация 6 — integration + acceptance

A–D багфиксы; E финальный demo + REPORT.md; оркестратор — acceptance по §5.

---

## 5. Критерии приёмки MVP (12 + 3 уточнения)

| # | Критерий | Проверяется |
|---|---|---|
| 1 | `cargo test --workspace` зелёный | CI |
| 2 | R1-AST зелёный на всех **47** golden AST | `runner --tier-a --relation r1` |
| 3 | **R1-text зелёный на #47** (единственный RAW-вектор Tier A); остальные Tier A — not applicable; Tier B — per-file по manifest (RAW-файлы) | `runner --relation r1-text` |
| 4 | R2 зелёный на всём Tier A | `runner --relation r2` |
| 5 | R3 зелёный на всём Tier A | `runner --relation r3` |
| 6 | R4 — все ожидаемые коды совпадают (включая негативы N1–N19 и warning-исходы) | `runner --relation r4` |
| 7 | Tier B: parse-or-diagnose 100%, ноль crash | `runner --tier-b` |
| 8 | Симуляция: EID → LinuxCNC sim → 0 errors, motion stream matches (или mock с пометкой) | `sim_check` |
| 9 | Demo: `translate --from grbl --to linuxcnc corpus/demo/pocket.gcode` → валидный `.ngc`, sim проходит | автопроверка |
| 10 | Demo reverse: LinuxCNC → GRBL → EID стабилен | автопроверка |
| 11 | REPORT.md — что работает, что нет, ограничения; **пометка: demo-равенство трёх AST — R1-цепочка через компилятор, не R3-on-Tier-B** | review |
| 12 | **CHANGELOG проверяется скриптом** (`tools/changelog_check.sh`): каждый PR к main имеет entry + bump версии затронутого документа + bump `eid_version` в manifest при изменении формата | CI-линт |

**Демо-сценарий:** pocket.gcode → translate → pocket.ngc → verify → sim → 0 errors → reverse → regenerated.gcode; `parse_grbl(pocket) ≡ parse_linuxcnc(pocket.ngc) ≡ parse_grbl(regenerated)` в допусках. Равенство легально: оба файла порождены из одного источника.

---

## 6. Риски и планы B

| Риск | Митигация |
|---|---|
| **R-1: `serde_yaml` (v0.1.1, ревью)** | maintenance mode с 2024; тянет `unsafe-libyaml` (unsafe-блоки) в бинарь CLI транзитивно. Компиляционный путь (core/grbl/linuxcnc) — чист, риск только в harness/integration. **Решение:** миграция на TOML (`toml` crate, без unsafe) для MVP; hand-parse фиксированной схемы профиля — цель v1.0 (zero-dep всего workspace как claim). **Дедлайн: до публикации внешних профилей** — формат профилей/manifest — точка невозврата после 50 профилей в дикой природе. Не блокирует итерацию 4 |
| Интерфейсы изменились после freeze | Только PR к оркестратору; rebase всех; v0.2-interface |
| **Агентские зависания** (v0.1.1) | Монолитные задачи → гигантская единичная генерация → тихое зависание бэкенда (~16 мин, нет запросов). Митигация: staged-брифы (маленькие порции, компиляция после каждого модуля) — обязательны для задач > одного модуля; мониторинг — свайп всех агентов на каждое событие + stall-репорты |
| A — узкое место | A закрыт в итерации 1; дальше только bugfix |
| E заблокирован на C/D | E начинает с profile/CLI; sim — mock до итерации 4 |
| Tier B медленный | Минимум 30 файлов для MVP; 50–100 — цель |
| R3 красный | Разбор по CORPUS §9 (4 категории, R3-декомпозиция consistency/golden); виновный НЕ априорен; >2–3 Spec-ambiguous → ревизия спеки до итерации 4 |
| Float-флаки | EQUIVALENCE §4 — единственный источник допусков; коэффициенты — только ревизией |
| Sim недоступен | MVP принимается с пометкой в REPORT.md |

---

## 7. Вне scope MVP

ИИ-харнесс (9), Fanuc (10), open-source release (11), хвост (12). MVP на 2 диалектах — доказательство архитектуры.

---

## 8. Итог одной фразой

**5 агентов (A: core, B: harness, C: GRBL, D: LinuxCNC, E: integration) работают параллельно после freeze v0.1; 6 итераций ведут к R3-зелёному на 47 векторах Tier A; оркестратор принимает MVP по 12 критериям, главный — demo «GRBL → LinuxCNC → GRBL, все три AST равны» (R1-цепочка, не R3-on-Tier-B).**
