# eid — G-code compiler: L2-макро → канонический EID/L1 → контроллеры станков

Транслятор/компилятор G-кода любого-в-любой:

* **L2 → L1 компиляция** — программы с макро-языком (переменные `#`,
  `IF`/`WHILE`/`GOTO`, вызовы `G65`/`G66`/`M98`, O-word подпрограммы
  LinuxCNC) **исполняются символьно-конкретным интерпретатором** и
  компилируются в concrete-опы канона — не текстовые носители;
* **каноническое промежуточное представление EID/L1** (resolved-state AST,
  10 опов, формально определённая эквивалентность);
* **4 контроллера + канонический формат** — GRBL 1.1, LinuxCNC, Fanuc,
  Haas (`eid-l1` — пятый, канон);
* **TCP→joints для 6 кинематик** — RTCP-профили станка: XYZAC/XYZBC
  table-rotary-tilting, XYZAB cradle/trunnion, head-head nutating,
  head-table intersecting, gantry XYZBCW — формулы ядер сверены с
  C-оракулами LinuxCNC, круговые инварианты ≤ 8e-13;
* **символьная верификация ветвлений (SMT-кодирование семантики #0)** —
  term-builder side-condition матчинг, без внешнего solver-депа;
* **дифференциальное тестирование на живом rs274** (LinuxCNC 2.9) —
  референс-оракул эквивалентности: 48 e2e-векторов 0 divergences,
  Fanuc Macro B trace-зеркала semantic_diff = 0.

Workspace: **1584 теста / 0 упавших** (`cargo test --workspace`).

## Что компилируется (L2-слой)

| Семейство | Парсер | Что понимается |
|---|---|---|
| Fanuc Macro B | `parser_fanuc` | `#1–#999` (+ системные #4xxx/#5xxx), арифметика с `[]`, `IF [..] GOTO/THEN`, `WHILE..DO/END`, `G65/G66/G67` (нелинейная Fanuc-таблица букв аргументов), `M98 P…L…`/`M99`, O-тела |
| Haas Macro B | `parser_haas` | ядро Fanuc + bare-IF `IF [..] #v=..` (wrap), `M97 P<блок>` (N-метки) |
| LinuxCNC O-word | `parser_linuxcnc` | `o<имя>`/`o123 sub/endsub/call`, `if/elseif/else/endif`, `do..while/endwhile`, `break/continue`, `#<_ini[..]>`/`#<_hal[..]>`, позиционные args call, выражения `ATAN/EXISTS/@r^a` |

Компиляция (`Pass-1 lowering`): переменные вычисляются, циклы
развёртываются, ветвления разрешаются → на выходе concrete L1-опы.
Непредставимое — честные RAW-носители с подставленными значениями и
именованной причиной («не молча»): дуги под RTCP, чужие ротационные
слова, vendor M-коды, state-only кадры.

## Кинематики (RTCP, фаза 12.5)

Секция `[kinematics]` TOML-профиля включает TCP→joints компиляцию
(G43.4/G43.5 — RTCP): ходы в TCP-координатах пересчитываются в machine-оси
через inverse-формулы ядра.

| Топология | Машины | Ядро | Оракул | Круговой инвариант |
|---|---|---|---|---|
| XYZAC table-rotary-tilting | trt-столы | `kinematics::xyzac` | LinuxCNC `trtfuncs.c` | 3136 кейсов, 2.27e-13 |
| XYZBC table-rotary-tilting | trt-столы | `kinematics::xyzbc` | LinuxCNC `trtfuncs.c` | тот же, 2.27e-13 |
| XYZAB cradle/trunnion (TT-2) | Hermle-класс | `kinematics_tdr` | `xyzab_tdr_kins.comp` | 784, 3.4e-13 |
| head-head nutating (ν 0–90°) | универсальные головы | `kinematics_trsrn` | `xyzacb_trsrn.comp` + **независимый python kins-twp**, She 2007 | 4116, 3.4e-13 |
| head-table intersecting | DMU 50-класс | `kinematics_maxkins` | `maxkins.c` | 1568, 8.0e-13 |
| gantry XYZBCW | bridge mill | `kinematics_5axis` | `5axiskins.c` | 1568, 1.1e-13 |

Семантика lowering per-топология: trt/tdr/maxkins/five-axis — pass-through
ротационных слов (программа содержит joint-значения) + TCP→joints; Trsrn —
solver (программные углы = ориентация инструмента, joints решаются
angle-solver'ом). Независимые верификации не от C-кода: python-оракул,
Родригес, численный якобиан, degenerate-сверки.

## Что работает end-to-end, что — library-only, что — отказ

Граница claim'а «6 топологий подключены» — точно по сценариям:

| Сценарий | Статус | Что происходит |
|---|---|---|
| 3-осевые программы (L1↔L1, любой контроллер) | ✅ end-to-end, verified | прежний конвейер — 157-файловый корпус, rs274-оракул |
| Macro B / O-word программы **без** ротационных ходов (fanuc/haas/linuxcnc) | ✅ end-to-end через CLI | макросы компилируются в concrete L1-опы; units-скан; RAW-носители → §8.2 |
| **Zero-rotary TCP** (программа в TCP-координатах под G43.4, ходы только XYZ) + профиль `[kinematics]` | ✅ end-to-end через CLI | inverse применяется на текущих (домашних) углах: TCP→joints — при реальной геометрии machine-значения ≠ TCP; G43.4/G49 материализуются комментариями шапки (исполняемая эмиссия G43.4 дала бы двойное преобразование — цель применила бы свою кинематику поверх наших joints) |
| Программы **с** ротационными ходами (A/B/C в кадрах) + профиль | ❌ именованный отказ (E_UNSUPPORTED_SEMANTICS) | L1-грамматика v0.3 не имеет слота ротационных осей (Op-концы Point3): TCP→joints lowering вычисляет X/Y/Z joints, но A/B/C в L1-текст не представимы — эмиссия потеряла бы движение ротационной оси |
| Полный 4/5-осевой lowering (в т.ч. с rotary) | ⚙️ library-only | `eid_l2::lowering::lower(&program, cfg)` с `MachineProfile::l2_lowering_config()` — круговые инварианты ядер; e2e `kinematics_lowering_e2e.rs`; НЕ через CLI |

Практический смысл: **«перенести деталь с 5-осевика на 5-осевик» через CLI
сейчас не работает** — реальная 5-осевая программа содержит rotary-движения,
и она получает отказ на эмиссии.

### Почему так: L1 AST не хранит A/B/C — это архитектурное решение, не баг

Корень — одна строка: L1 `Op` (Rapid/Linear/Arc) хранит концы как `Point3`
(x, y, z) — **осознанный замороженный дизайн** этапа 6 (3-осевой канон),
не забытый слот. L2 несёт полную позу (`ExprPose { x, y, z, a, b, c }`);
kinematics inverse **работает** (6 топологий verified, круги ≤ 8e-13) —
но его результат для rotary-хода содержит **ненулевые joint-углы**, а
положить их в L1 **некуда**:

| Сценарий | Inverse | Запись в Point3 |
|---|---|---|
| Zero-rotary TCP | углы домашние → ротационные joints = 0 | XYZ описывают состояние машины **полно** (при ненулевой геометрии XYZ ≠ TCP — в этом смысл компиляции) — **терять нечего**, работает |
| Rotary | ротационные joints ≠ 0 | A/B/C **некуда положить** — эмиссия потеряла бы движение оси → именованный отказ |

Разница — не в кинематике, а в том, **есть ли что терять при записи в
Point3**. Кинематика готова; не подключена **трансмиссия** результата.
Расчёт 4/5 осей — в L2 (факт); трансмиссия = **второй маршрут эмиссии
поверх lowering** (не «патч L1»). Три пути — ни один не начат:

| Путь | Суть | Статус |
|---|---|---|
| L1 v0.4 | расширить `Point3` до полной позы | не выбран — ломает freeze грамматики (все эмиттеры/тесты) |
| **L2→dialect напрямую («перевод поверх»)** | второй маршрут эмиссии: LoweringResult + rotary-каналы per-move → текст с буквами осей топологии из профиля `[kinematics]`; форматирование a/b/c-слов уже есть (emit_l2 `emit_move`) | **не начат; ближайший** — линейный срез (Rapid/Linear, same-topology) — **дни**; полный перенос — **недели**: арки под кинематикой требуют сегментации TCP-дуги в joint-полилинию (tolerance-решение, постпроцессорный класс), feed в joint-space без RTCP — семантическое решение, кросс-топология — two-kins пайплайн (source-интерпретация углов + target-inverse); оракула для rotary-вывода нет (trace валидирует макро-семантику, круг-инварианты — ядро, не эмиссию) — приёмка постпроцессорный дифф/станок |
| L1.5/L3 | промежуточный уровень с rotary | не выбран — новый слой, самый дорогой |

Почему это **правильно**: L1 — **пол** — простым, замороженным,
zero-dep и исполняемым на GRBL/LinuxCNC; расширь его до 6 осей — он
перестанет быть полом. Кинематика — **в L2**, потому что она сложная
(inverse, сингулярности, топологии) и не обязана быть исполняемой.
Rotary-эмиссия — **отдельный слой**, не нужный 3-осевому цеху: 3-осевой
цех **не платит** за сложность 5-осевого.

Семантика профиля (решение 12.5.3, «полный-TCP скоуп»): активный
`[kinematics]` = программа трактуется как говорящая TCP-координатами
(семантика RTCP-контура); plain machine-coords программу под этот профиль
подавать нельзя. Per-op диагностика называет профиль и семантику («не
молча»). Флагманский тест честно: «zero-rotary TCP под активной
кинематикой — inverse на домашних углах, end-to-end, кинематика не
ломает 3-осевые программы»; это НЕ «5-осевая трансляция».

**Честная оговорка об оракуле**: круговой инвариант (pose → inverse →
forward → pose) проверяет само-согласованность формул, но не эквивалентность
реальному контроллеру (rs274-оракул 5-осей не существует — RTCP исполняется
реалтайм-слоем LinuxCNC). Настоящая верификация — постпроцессорный дифф
конкретной машины или физический станок (перспектива при доступе).

## CLI

```
# L1↔L1: GRBL → Fanuc (--profile опционален: дефолт по цели)
eid translate --from grbl --to fanuc --input part.ngc --output part.nc

# L2-маршрут: Macro B компилируется (переменные/циклы resolved в concrete)
eid translate --from fanuc --to linuxcnc --input macro_part.nc --output out.ngc

# TCP→joints: zero-rotary TCP-программа + профиль [kinematics]
# (G43.4 → комментарии шапки; ходы с A/B/C под профилем — именованный отказ,
#  см. «Что работает end-to-end»)
eid translate --from fanuc --to grbl --profile 5axis_xyzac.toml --input tcp_part.nc --output joints.ngc

# verify: parse + validate + mock-sim; --dialect вместо полного профиля
eid verify --dialect linuxcnc --input part.ngc
```

Диагностики — с этапом возникновения (`[parse] E_INVALID_INPUT: …` @
строка:колонка); отдельно печатаются drop-notices (SPEC §3), emit-warnings
(Q3) и lowering-диагностики (носители, RTCP-материализация, авто-PEND).
Exit-коды: 0 ok / 1 отказ (диагностики трансляции, pending-адаптер, лимиты
профиля) / 2 usage.

Единицы: L2-маршрут сканирует G20/G21 (в L2-AST единицы абсорбируются
парсером) — шапка эмиссии честная; units-switch (оба кода в программе) —
отказ (конверсия анкером есть только у L1-адаптера linuxcnc). NURBS G5.x
(SPEC v0.19) маршрутом L2 не перехватывается — line-level Raw обрабатывает
адаптер linuxcnc.

## Архитектура

```
 GRBL 1.1 ─┐  parse → canonicalize → verify          emit + legality  ┌─→ GRBL 1.1
 LinuxCNC ─┤  (modal state → resolved-state AST;                      ├─→ LinuxCNC
 Fanuc    ─┤   RAW двух классов: same-dialect verbatim,        EID/L1 ┼─→ Fanuc
 Haas     ─┤   кросс-диалект — E_RAW_UNTRANSLATABLE)     детерминир. ├─→ Haas
 EID/L1   ─┘                                                        └─→ EID/L1
     ▲            ▲
     │   L2-слой: parser_fanuc/haas/linuxcnc → L2Evaluator (D0–D3,
     │   макро-семантика #0) → Pass-1 lowering → concrete L1-опы
     │   (+ kinematics: TCP→joints, 6 топологий; + SMT side-conditions)
     │
  L2↔L2 эмиссия (12.4): Macro B Fanuc↔Haas, O-word LinuxCNC
  (GOTO → o-if/do-while реструктуризация)
     ▲
  rs274 (LinuxCNC 2.9, Docker) — референс-оракул эквивалентности
  (+ Fanuc Macro B trace-зеркала через L2-драйверы, semantic_diff = 0)
```

Шесть машин трансляции вокруг одного канона. Каждый диалект — пара
parse↔emit; emit-путь guarded legality-таблицей (typed-enum
LegalityCondition: legal/illegal + коды как данные — эмиттер отказывает,
а не выдумывает код). Каноническое ядро — `eid-core`: AST, verify-pass
(каждая диагностика помечена этапом: Parse/Canonicalize/Verify/Emit),
текстовый формат.

## Верификационный стек

1. **Корпус** — 157 файлов Tier B: свеп 116 Ok / 41 именованных Err
   (81 класс отказов: 48 → Ok-кандидаты задокументированы, 17 TRUE-ERR,
   4 AMB); Tier A — 68 векторов (48 позитивных с golden + негативы).
   Два контура лицензирования: published (MIT) vs local-dev (GPL,
   gitignored). Аудит: 131/157 чисто 3-осевых, все скрытые классы
   (ротационные/RTCP/NURBS/трансформации) — named dispositions.
2. **Референс-оракул rs274** — 48 позитивных векторов e2e: 0 divergences
   (motion stream, semantic_diff). Docker-контур; без Docker — SKIP, не
   красные.
3. **Trace-эквивалентность L2** — Fanuc Macro B файлы: native-(a) ↔
   flat-(б) через L2-драйверы → rs274 → semantic_diff = 0 (9/9);
   BUG-D1 (нелинейная таблица G65-букв D→#7, F→#9) пойман зеркалом.
4. **Круговые инварианты кинематик** — forward∘inverse ≡ identity на
   решётках углов (числа в таблице выше); trsrn solver round-trip
   200/200 ориентаций.
5. **SMT-кодирование** — side-condition матчинг ветвлений сверен
   L2-семантикой ошибок (div/LN-пины); term-builder без внешних solver'ов.
6. **Мутационный фаззер + extended fuzz** — M-петля 20 seeds × 17
   операторов; byte-loop категорий 21 × адаптеров 2; новых классов — 0.
7. **CI** — fmt + clippy `-D warnings` на весь workspace.

## Trust story

**Ноль внешних зависимостей на пути, который генерирует твой G-код.**
Все трансляционные крейты — `eid-core`, `eid-grbl`, `eid-linuxcnc`,
`eid-fanuc`, `eid-haas`, `eid-l2` — std-only: единственные зависимости —
path-deps на `eid-core` (у него ноль зависимостей вовсе); проверяется
`Cargo.toml`. Вне трансляционного пути — два сервисных контура (serde+toml):
профили/реестр (`eid-integration`) и QA-инфраструктура (`eid-harness`).
Формат версионируется (`grammar.ebnf` v0.4; magic-строка каждого
EID-файла = версия формата). Эквивалентность формально определена
(`EQUIVALENCE.md`): что сравнивается, что не входит, все допуски —
с фиксированными коэффициентами, меняющимися только ревизией.

## Структура

```
crates/
  eid-core/         AST EID/L1, диагностика, verify-pass, EID parser/emitter  (std-only)
  eid-grbl/         GRBL 1.1: parse ↔ emit                                    (std-only)
  eid-linuxcnc/     LinuxCNC: parse ↔ emit, NURBS spline_active, канон циклов (std-only)
  eid-fanuc/        Fanuc: parse ↔ emit, Macro B → Raw, DPI, EOB `;`          (std-only)
  eid-haas/         Haas: parse ↔ emit, WCS p=7..9 → G110–G112, P-dot-rule    (std-only)
  eid-l2/           L2: парсеры Macro B/O-word, интерпретатор D0–D3, Pass-1
                    lowering, RTCP-ядра 6 топологий, L2↔L2 emit, SMT (std-only)
  eid-harness/      semantic_diff, corpus manifest, генератор/мутаторы/фаззер
  eid-integration/  реестр адаптеров, machine profiles (TOML + [kinematics]),
                    L2-маршрут translate, sim (mock + rs274 real)
  eid-cli/          translate / verify
corpus/             Tier A (68 векторов: 48 позитивных с golden + 20 негативов)
                    + Tier B (157 файлов) + manifest (см. CORPUS-CONTRACT.md)
docs → *.md         см. ниже
```

## Лицензии корпуса

Корпус — два контура (CORPUS-CONTRACT §7; manifest фиксирует sha256 и
лицензию каждого файла): **published** — сэмплы с разрешающей лицензией
(MIT), только они формируют публичные claims; **local-dev** (GPL/неясная
лицензия) — тест-only контур, в публикацию не входит. Tier A — собственные
векторы проекта (golden AST + пары диалектных файлов).

## Документы (версионированы, журнал — CHANGELOG.md)

| Документ | Что это |
|---|---|
| `SPEC-EID-L1.md` (v0.20 + §2.5-ревизия v0.21) | Спека промежуточного диалекта: модель, 10 опов, инварианты, §7.7-шаблон циклов, G92-mid/bare-T/PEND-хвост по зондам rs274 |
| `EID-L2-DESIGN.md` (v0.1 + амендменты v0.1.1–v0.1.6) | Дизайн L2-этапа: AST §2.4, семантика #0, матрица §3, кинематики §12.5 |
| `COVERAGE.md` (v0.12) | Матрица диспозиций G/M-кодов 6 диалектов (GRBL 1.1, grblHAL, FluidNC, LinuxCNC, Fanuc, Haas) + реестр `?` (72 поз.) |
| `EQUIVALENCE.md` (v0.3.1) | Контракт эквивалентности: фреймы, допуски, исключения |
| `CORPUS-CONTRACT.md` (v0.7.1) | Корпус: тиры Tier A/Tier B, протокол классификации красного |
| `CONSTRUCT-REGISTRY.md` (v0.0) | Навигационный срез «конструкция × диалект» |
| `INTERFACE-V0.2-DESIGN.md` | Разморозка API v0.2 (5 волн, решения на измерениях аллокаций) |
| `grammar.ebnf` (v0.4) | Авторитетный текстовый формат EID (D1–D10) |
| `kinematics_research.md` | Исследование топологий: оракулы LinuxCNC-kins, формулы, план 12.5 |
| `IMPLEMENTATION-PLAN.md` (v0.1.3) | 5 агентов + оркестратор, итерации, приёмка |

## Сборка

```
cargo test --workspace
```

CI: fmt + clippy gates. rs274-референс — опциональный Docker-контур
(`docker/linuxcnc-sim.Dockerfile`); без Docker соответствующие тесты SKIP
(не красные).
