# REPORT — MVP-цикл, итерации 0–5

**Дата:** 2026-11-22 · **Оркестратор:** сессия по плану `IMPLEMENTATION-PLAN.md` v0.1.3 (5 агентов: A core, B harness/corpus, C GRBL, D LinuxCNC, E integration; оркестратор — координация, классификация, acceptance)
**Версии:** SPEC v0.20 (+§2.5-принцип 5 ревизия v0.21) · grammar v0.4 (back-compat, magic v0.3) · EQUIVALENCE v0.3.1 · COVERAGE v0.12 (6 диалект-колонок) · CORPUS v0.7.1 · PLAN v0.1.3 · interface v0.2.0 · **manifest: 68 векторов + 157 Tier B (70 published / 87 local-dev); 17 fanuc + 13 haas-dialect → live через зарегистрированные адаптеры**

---

## 1. Статус: что построено

**Транслятор 5 живых диалектов через канонический EID/L1**: grbl-1.1, linuxcnc, fanuc, haas, eid-l1 (pending: grblHAL, FluidNC — COVERAGE-колонки готовы). **Архитектурный claim «N+2, не N²» — эмпирическое доказательство**: 5 парсеров + 5 эмиттеров = 10 адаптеров вместо 20 P2P-конвертеров; добавление 6-го диалекта = 2 новые поверхности (не 10 конвертеров) — факт, не гипотеза: три диалекта добавлены после MVP (fanuc/haas/eid-l1-модификации) без изменения ядра.

**Named evidence — три раздельных claim'а** (корпус-аудит + CLI-пробы, 2026-11-22):
- **Claim «семантика 3-осевой фрезеровки покрыта полностью»** — код+тесты: 3D-координаты (X/Y/Z везде), дуги G17/G18/G19 (вкл. отрицательные смещения, audit #34), helical, полные окружности v0.18. «3D-рельеф» — не отдельная семантика, а те же линейные ходы с меняющимся Z; объём — вопрос размера, не класса.
- **Claim «any-to-any на диких файлах»** — пробы: `mit_tball_tball.gcode` (2680 опов, 2.5D-контур) — кросс grbl→linuxcnc→grbl **байт-идентичен первому раунду**; `mit_shpk_MIT_EC_Logo.nc` (50KB) — кросс **back == same-dialect-round байт-в-байт**; `olle_bricka_short_wall.tap` (**49 Z-градаций** — Z-рельеф-класс) и `gcb_TEST_MillHolesGrid` (54KB, 24 Z) — same-dialect fixed-point ✓.
- **Claim «граница 4/5-axis названа и измерена»** — корпус-аудит 157 файлов: **131 чисто 3-осевых (83.4%)**, 26 с именованными скрытыми классами (ротационные 11, RTCP/TWP 1, трансформации 5, полярные 2, NURBS 15 — multi-tag; все Raw с диспозицией). `vis_SmilinJack` — обнаружен аудитом как **фактически 4-осевой** (A-слова: 3 Raw, parse+same-dialect ✓, кросс-отказ честен); `ncm_g434_rtcp_5x` — **99.98% Raw** by design (same-dialect verbatim ✓). Прежний «SmilinJack = 3D-контур» — ложное evidence, поймано проверкой до публикации.

Каждый адаптер: парсер с канонизацией (modal state → resolved-state AST), детерминированный эмиттер с legality-таблицей, **развёртывание canned cycles по §7.7** (4 поверхности развёртки на единых v0.18-константах, rs274-референс 0 divergences), RAW двух классов (same-dialect verbatim / кросс-диалект отказ), verify-проход со stage-атрибуцией, semantic_diff по допускам EQUIVALENCE §4; runner R1–R4; Tier A 68 векторов + Tier B 157 файлов (свип Ok 116/157 после v0.20-волны); CLI (translate/verify, живой режим); профили с emission-опциями; **sim real (LinuxCNC 2.9 rs274 в Docker) + sim mock**.

**Числа:** **1183 теста passed / 0 failed** · 100+ коммитов · SPEC v0.20 (20+ ревизий; §2.5-принцип 5 ревизия v0.21 — units-switch mid-program легален) · аудит 36 пунктов: все обработаны · Tier A R3 **83/0 golden + 35/0 consistency** · Tier B **157 crash=0, свип Ok 116/157 после волны-2** · **rs274 canonical comparison 48/48 — 0 divergences** · mutation kill-rate изменённых поверхностей 80.5% (fanuc 90.3%) · CI fmt+clippy gates зелёные.

**Временная формулировка (честная):** финальный push — итерации 7–10 за 4 дня; проект — от пустой директории до 1115 тестов через 16+ ревизий спеки, 4 СТОП-протокола (агент→оркестратор, все корректные) и 10 формально закрытых итераций — быстро, но не за счёт пропущенных шагов.

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

- **NaN-swallow resid-проверок — ИЗВЕСТНОЕ ОГРАНИЧЕНИЕ, не блокер** (классифицировано итерацией 11 по решению ревью): сравнения с NaN в IEEE-754 ложны → resid-проверка уравнения позиции дуги вырождается в молчаливый pass на NaN-промежуточных (EQUIVALENCE §4 заметка; 29 мутантов-выживших — ТОЛЬКО синтетика мутаций). **Почему NaN не появляется на реальных данных**: лексер принимает только digit/dot/минус (текст физически не может дать NaN/inf), числа ограничены диалектными лимитами (9 цифр + 4dp), арифметика на ограниченных значениях остаётся finite (максимум ~3e10 в G93-разрешении — далеко от f64-потолка). **Классификация: не блокер** — но is_finite-гейт в semantic_diff/verify остаётся кандидатом v0.3.x при первом реальном кейсе (дешёвая страховка, применить легко).
- **Симуляция**: criterion #8 ЗАКРЫТ (canonical comparison 48/48, 0 divergences); остаток — motion-level компаратор для Tier B Ok-файлов с чувствительностью к командному порядку (закрыт для #25-класса в v0.18 F-правилом; остаток — конкурентный порядок на циклах, демо parallel parity бит-в-байт).
- **Manifest-диалекты в sweep**: `dialect_of` — эвристика по префиксу, синхронизирована с manifest вручную; кандидат — читать диалект из manifest.toml напрямую.
- **Tier B published — 2D/2.5D-скew снят** (157 файлов: construct-батч + haas-добор); сироты документированы (CONSTRUCT-REGISTRY + haas-отчёт §сироты).
- **Q11: unknown-start** — канонизатор отказывает partial-end после genuinely-unknown (v0.11) — 3 published-файла диагностируются корректно; апгрейд «дефолт-наследование» — кандидат ревизии.
- **v0/v1-долг §1**: закрыт регенерацией (70/76 пар v1; 4 diverged — классифицированы и починены как Adapter-wrong C/D; #41/#45 grbl — lossy навсегда по Q3). Независимость v0-пар как теста корректности сохранена (v0 = regression-корпус).
- **grblHAL/FluidNC** — адаптеры pending (флаги профиля), коллекция NOASSERTION-источников не велась — post-MVP.

## 5. Процессные итоги (работа системы 5 агентов + оркестратор)

- **Самокоррекция сработала 4 раза**: D→оркестратор (#39 контр-классификация), C→спека (§2.5 units), B→сам (#19 геометрия), оркестратор→себя (#48 File-wrong, счётчики 67/58/48).
- **Все правила в документах**: ноль «код-без-спеки» долгов после v0.13-аудита (спека v0.13→v0.15 догнала код).
- **Режимы**: staged-брифы против зависаний; прицельные коммиты; активная версия в шапке задач; documents-only-via-editor.
- **Цикл классификации красного** (CORPUS §9): 70 красных v1 → 4 категории → адресные фиксы → 0 красных v5-v7 за две итерации.

## 6. Итерация 6 — Fanuc (этап 10) + волна аудита (36 пунктов) — ЗАВЕРШЕНЫ

### 6.1 Fanuc-адаптер — ПОЛНЫЙ ЦИКЛ (этапы 1–3, коммиты c5c2ac8/421678c/a49a6e9)

- **Этап 1** (парсер): лексер с EOB `;`-разделением (не комментарий — ловушка №2), DPI-политика (bare-integer × least-increment + W_DPI_ASSUMED; F исключён из ×0.001 — ревью нашло баг брифа: Fanuc F-инкремент = 1 mm/min, параметр 1422), модальные группы + G70/G71-псевдонимы, движение G0–G3 все формы. 68 тестов.
- **Этап 2**: фреймы (G54–G59, G10 L2, G92±S, G53, **G52 = ERR** — единственный жёсткий отказ, G10 L20 ≠ wcs_set_here — ловушка №3, G54.1/G92.1–.3), циклы ×7 по **Fanuc-семантике** (P-мс, δ=0.5mm+W_CYCLE_PARAM_ASSUMED, K/L-повторы, G86-рестарт), M-словарь, Macro B → line-level Raw, G93 ABSORB, D9 round-trip фрагментов (CRLF). 147 тестов.
- **Этап 3** (эмиссия): FanucEmissionLegality (ConversionTarget), decimal-point-всегда (DPI-ловушка golden), dwell P-мс/X-правило, полный круг без end-слов, path_mode exact_stop→G61 / exact_path→G61+warn / blend→G64+warn, M30 дефолт, wcs_set_here → отказ (честный), зеркала SPEC v0.17, **Raw-хвост после PEND легален** (DNC-subs файлы). 203 теста. Смоук 17 fanuc-файлов корпуса: crash=0, ok=6 (включая fixed-point на 44 618-оперном RTCP-файле) / diagnosed=11 честно.
- COVERAGE: v0.8 Fanuc-колонка (44 строки + 64 ячейки, Q6 закрыт, реестр +16) → v0.8.1 (rulings) → v0.9 Haas-колонка (36 строк + 101 ячейка, коллизии ≠Fanuc, реестр 66).

### 6.2 Внешний аудит 36 пунктов — все обработаны (зонд-дисциплина)

Каждое ключевое заявление проверено **зондами реального rs274** до фикса: #15 подтверждён (`ARC_FEED rotation=-1` — реальный контроллер режет полный круг с явными X Y, мы молча дропали), #18 подтверждён (rs274 принимает пробел в слове), **#17 ОПРОВЕРГНУТ** (rs274 тоже отвергает вложенные комментарии — наша строгость честна), #1 ложная тревога (Cargo.lock показывает и dev-рёбра; harness уже dev-dep).

- **Критические закрыты**: #33 — модальная абсорбция на RAW-строках (`G91 G28 Z0` терял G91 → следующий кадр молча абсолютный; SPEC §8.6); #15 — полный круг 360° по геометрии (SPEC §3 v0.17 пересмотрел v0.13-правило «формы команды»; CORPUS #13 re-disposition, goldens regen); #29/#30 — тихие u32-обрезания (T4294967296→T0, P1.9→P1).
- **Спеки**: SPEC v0.17 (§3, §8.6, канон-нормаль pinned) · grammar v0.4 = back-compatible superset (op_num 3+ цифр, quoted-экранирование; **magic «EID/L1 v0.3» не менялся** — все goldens целы).
- **Зоны**: A (eid-core: feed>0, eps_pos, raw_refusal→eid_core::diag, clippy+fmt чисто), B (harness: cfg(windows)-docker-пути, .out-гард ДО прогона — 288 утечек→0, generator tan-clamp, cargo-fuzz targets), D (13 пунктов eid-linuxcnc, 138 тестов), E (CLI --profile опционален, профиль в конвейере, Fanuc=AdapterPending, units×25.4, CI fmt+clippy gates), grbl-зеркало (117 тестов).
- **Корпус-последствия**: manifest 144 (конструкционный добор 23 файла, лицензии независимо верифицированы; RTCP 2.1MB/G69 2.5MB cap-исключения приняты — единственные публичные сэмплы); **N11/N14 expect-ревизия** (SPEC v0.16 supersede CORPUS v0.5.2: auto-PEND → Ok+notice, M98 → Ok+Raw); tier_b тесты → manifest-динамика (хардкод 92 убран).
- **Финал волны**: **workspace 758/0**, R1 48/0 · R2 48/0 · **R3-golden 83/0 · R3-consistency 35/0** (оба красных #13 очищены grbl-зеркалом, stash-верифицировано) · R1-text 8/0 · R4 8/0 · Tier B 144 crash=0 · rs274_e2e48: #13 [ok] 0 divergences live · tree-wide fmt применён (CI fmt-gate зелёный).

### 6.3 Референс-сверка циклов — 21 red → 0 (SPEC v0.18)

Классификация 21 rs274-e2e48-красного (Agent D, CORPUS §9) дала **14 GeoDiff**: SPEC §7.4a фиксировал константы «сам», живой rs274 показал другие (зонд-подтверждено): клиренс пека **0.01in unit-scaled** (не 0.5mm/«0.05in»), **G73 = G83 без полного отвода** (одиночный rapid d_(k−1)+0.254; модели min(R,d+Q) в интерпретаторе нет, −2 опа/цикл), **G86 P обязателен + DWELL-оп**. OrderDiff-гипотеза опровергнута (порядок 1:1); зелёные соседи (#17/#18/#22–#26) доказали базовый шаблон §7.7. **SPEC v0.18** (4abaf2f) → реализация D (4705480) → grbl-зеркало C (b7d695e: GRBL 1.1 без canned cycles в словаре — пин диспозиции; F-правило эмиссии) → **rs274_e2e48: 21 red → 0** (вектор #21: +P1.0 File-wrong, −restated-F по ruling b; AST-флаг F-кадра — в очередь разморозки v0.2). COVERAGE §6.14 ЗАКРЫТ (v0.9.2); §6.19: Haas Setting 22 ≠ канон — диалектное расхождение зафиксировано.

### 6.4 Отложено осознанно (не потеряно)

#21 SrcSpan→Arc<str> · #22 AdapterOutput · Box-PR large_enum_variant · **AST-флаг F-кадра** — все в очереди разморозки API v0.2 (бласт-радиус через замороженный интерфейс v0.1) · NURBS/Q10 — ревью-очередь пользователя.

## 7. Итерация 7 — Haas-адаптер + fanuc-live + дизайн v0.2

### 7.1 eid-haas — ПОЛНЫЙ ЦИКЛ (этапы 1–3, коммиты 477bc9e/73ab099/93b3bda)

Парсер + эмиссия Haas NGC: **176 тестов**, зарегистрирован live (5-й адаптер реестра). Ловушки контракта все поглощены: DPI machinists-notation с **F-доменом ×0.0001 (Setting 77)** — собственный тест поймал начальную unit-scaled ошибку; EOB `;`; **G52 = Err** (2-й вендорский отказ); G10 L2 P0 = слот G52 (**≠ Fanuc external offset**), G10 L20 ≠ wcs_set_here; **G70/G71/G72 = болтовые** (≠ Fanuc units); **P-dot-rule** (dotted=сек/bare=мс, уникальное Haas-правило) в G04 и циклах; **L-повторы (НЕ K)**, G83 переменные I/J/K (d1=I, dk=max(K,d−J)); **WCS-слоты G110–G112 → p=7..9 + W_WCS_EXTENDED** (нативная симметрия EID); M-коллизии ≠Fanuc запинены (M07=Shower, M29=relay, M13=тормоз 5-й оси). **Cross-check semantic_diff=0 на #19/#20/#21 — 4-я поверхность развёртки подтвердила SPEC v0.18.** Legality-enum с первого дня (INTERFACE-V0.2 волна-4: тотальные функции, ноль panic на emit-пути — первый адаптер на новом стандарте; старые — миграция в v0.2).

### 7.2 fanuc live + реестр

Регистрация "fanuc" (agent E, ff090bd): **17/17 fanuc-файлов корпуса обрабатываются живым адаптером** (ok=6/diag=11/crash=0 — паритет смоука), pending-тесты заменены живыми (harness +1, integration +2, cli +3). Находки: все ok-файлы содержат Macro B-Raw → кросс-диалект честно E_RAW_UNTRANSLATABLE (SPEC §8.1; same-dialect verbatim — единственный live-позитив); path_mode value-shift grbl-ExactPath→G61→ExactStop задокументирован (точная эквивалентность на G61.1).

### 7.3 INTERFACE-V0.2-DESIGN (agent A, 1dcdcb0) — решения на измерениях

Counting-аллокатор на 150k-оповой программе опроверг два claim'а: **SrcSpan→Arc — ДЕЛАТЬ** (file-String = 7.9% аллокаций, доминирует Num.text; «3–5×» — миф), **Box-PR — ОТКЛОНЁН** (+300k аллокаций/+21% на горячей клон-фазе, lint-зазор остаётся). AdapterOutput (Send+Sync) — ДЕЛАТЬ (открывает параллельный корпус/веб; фиксирует класс «затёртых notices» из R3-consistency). **F-флаг = State.feed_restated: bool** (88B/0-alloc, goldens целы; comparator-collapse отвергнут повторно — 3 основания). LegalityCondition-enum — ДЕЛАТЬ (паник-точки выписаны). План 5 волн + риск-реестр 12 + приёмка T1–T12.

### 7.4 Мутационный раунд v0.18 (agent D, 85e7054) — ЗАВЕРШЁН

Kill-rate изменённых поверхностей **75.8% → 80.5%** (1766 мутантов; core verify.rs 92.8%, grbl emit 92.3%, linuxcnc 75.8%, fanuc 78.4% — приоритет следующего раунда). 29 новых + 3 tightened теста убили 80 мутантов. **Real-gap = 0** — новых багов против SPEC не найдено (v0.18-код чист). ~70 equivalent классифицировано; ~207 missing-test каталогизировано. Наблюдение: fanuc sub-нумерация с дырами после zero-length-drop — кандидат SPEC §11-рулинга. Рекомендация: _tierb_full за cargo-фичу (docker 551с/запуск несовместим с mutation runs).

## 8. Итерация 8 — INTERFACE v0.2.0 + референс-верификация реестров — ЗАВЕРШЕНА

### 8.1 INTERFACE v0.1.2 → v0.2.0 (agent A, волны 1–4, 53a4b79)

Все 4 волны дизайн-дока реализованы, **992/0**, все оракулы зелёные (R1 48/0 · R2 48/0 · R3 83/0+35/0 · R1-text 8/0 · R4 8/0 · **rs274_e2e48 0 red** · goldens байт-целы · eid_version v0.3 не менялся · clippy -D зелёный):
- **W1 SrcSpan→Arc<str>**: сборка 150k опов −7.9% аллокаций (1.91M→1.76M), kept-pass −10.6% (числа дизайн-дока подтверждены до байта; shared-handle паттерн; anti-паттерн unique-name = 2.06M зафиксирован)
- **W2 AdapterOutput**: ParseOutput/EmitOutput, `trait Adapter: Send + Sync` (static-canary ×4, Arc<dyn Adapter> в core), RefCell удалён из 5 адаптеров, NoticeKind-супермножество 9 вариантов — «затёртые notices» класс закрыт; **параллельный прогон корпуса/веб-сервис открыты**
- **W3 State.feed_restated: bool** (88B/0-alloc): F-правило changed∨first_derived∨restated во всех 4 эмиттерах; **дыра e2e48#21 закрыта типом** (restated-F переизлучается; haas-тест обновлён — прежний ассертил lossy)
- **W4 LegalityCondition typed-enum во всех 4 эмиттерах** (тотальный code(), 0 panic на emit-пути; legality_table_complete)
- Отклонения от дока — 7 задокументированных решений по реальности (NoticeKind 9 ≠ 7, sub=1-эвристика оставлена fallback по §2.4, волна 4 накрыла fanuc)

### 8.2 Референс-верификация `?`-реестров (agent D, 809ff9c + 535b50b)

**20 `?` снято** с верифицируемыми источниками (полный NGC Mill Manual 2023 ch.4–19 через web-reader, haas.co.uk, gnea/grbl source-check, Zappettini/Smid), **3 атрибуции опровергнуты** (Setting 129 НЕ существует — gap 121–129; G61.1 нет в 30i-словаре; Setting 118 = «M99 Bumps M30 Cntrs»), ~28 сужены. **§6.68 GRBL-space = КОНВЕРГЕНЦИЯ, не баг** (real grbl отбрасывает whitespace на protocol.c-уровне — наш лексер конвергентен). 6 находок против доков применены (M6-Haas применяет offsets автоматически NGC-default; G76-Haas Setting 27 официально). Цитаты/URL/даты — registry_verification_notes.md.

### 8.3 Прочее

README v0.2 (ffa1a11): 5-диалектная статус-таблица, trust-claim верифицирован по всем 8 Cargo.toml. rs274-sim cargo-фича (58b9d7a): docker-тесты вне дефолт-прогона — mutation runs совместимы (551с-зависания устранены).

## 9. Итерация 9 — мутраунд-2 + emission-профили + parallel corpus + haas-корпус — ЗАВЕРШЕНА

### 9.1 fanuc-мутраунд раунд 2 (agent D, цель ≥85% превысена)

Kill-rate fanuc **76.4% → 90.3%** (1150 мутантов на v0.2.0-коде), **81 тест mround2_*** (raw-таблица g_raw_disposition целиком, absorb §8.6, пек-последовательности точные Z, sub-нумерация, R-формы, Родригес G18/G19, лимиты «ровно на границе» — 200000 опов конструктивно, validate-arms по текстам сообщений). **Real-gap = 0**. Наблюдения: NaN-swallow resid-проверок дуг системен для всех адаптеров (не баг — невалид отсекается раньше; кандидат EQUIVALENCE-заметка); dead-code takes_p/takes_q — кандидат очистки.

### 9.2 Emission-профили этапа 8 (agent C, 2a365cc)

`program_end_style` M30|M02 (validate отвергает M02 на grbl/linuxcnc — тихое игнорирование запрещено) · `wcs_extended_map` (таблица соответствия G59.x→G54.1/G154 с валидацией доменов P48/P99, notice-once «таблица подменена», неполная мапа → агрегированный отказ; fanuc map-on reparse=Raw BY DESIGN) · `allow_r_arcs` (через существующее emit_r_arcs v0.1; full-circle всегда IJK; R-roundtrip ≡ IJK semantic_diff=0). Механика: ProfileEmissionOpts в NamedAdapter-обёртках — **trait v0.2 не тронут**. 32 теста.

### 9.3 R10 — параллельный корпус (agent E, 2f7f778)

Send+Sync применён: `run_tier_b_parallel` (std::thread::scope + AtomicUsize-очередь, **0 новых deps**). **Parity бит-в-байт** seq↔par(16) на полном Tier B; скорость честно: мелкие ×4.06, большие ×2.10 (Амдал: SmilinJack 3.5MB — критический путь). Arc-SHARED паттерн (Arc::as_ptr identity в 8 потоках).

### 9.4 Документы и корпус

**SPEC v0.18.1** (9db50fa): §11 sub_index-нумерация после drop — дыры = documented semantics (мутраунд-1 наблюдение доведено до рулинга). **COVERAGE v0.9.5** (9b714f2): G76-Haas официальная XOR-механика (codes-G76 detail впервые через Cloudflare; Q→Setting 27 = Fanuc-5101-класс; без Q → I/J в тексте; этап-12-кандидат «всё в тексте» снят), §6.71: R = geometry-ЗНАЧЕНИЕ в G10 L10, код-гэп eid-fanuc локализован (кандидат consumed.extend — этап 12/бриф). **Haas-корпус** (c702501): 13 файлов (1 MIT + 12 NOASSERTION→local; пермиссивного живого Haas-NGC на GitHub нет), **dialect=haas** — 5-й ключ манифеста, живые через HaasAdapter; G31-сирота и H-до-Z закрыты дикими файлами. Корпус: **157 файлов**.

## 10. Итерация 10 — добивающая (dead-code, §6.71, NaN-заметка, emission-гейт) — ЗАВЕРШЕНА

- **§6.71 ЗАКРЫТ** (agent C, 129e639): consumed.extend P/L/R (зеркало haas) — официальный `G10 L10 G90 P5 R250.` → Raw{TABLE} вместо Err «R without consumer»; брифовское ожидание регресса (Err) исправлено эмпирикой (L2 P1.9 = Ok(Raw{SYNTAX}) и до и после — пин реальности).
- **Dead-code зачистка** (agent C): takes_p/takes_q, FeedPerRev-arm, k>MAX dead-check, end_is_last-пломбинг удалены; plane fallback-arms → unreachable! с инвариантом. **Мут-раунд 3: state.rs 254 мутанта — 0 missed** (все 7 эквивалентов ушли из счётчика), canonicalize 0 новых выживших. 315 тестов fanuc.
- **NaN-swallow EQUIVALENCE-заметка** (agent D, a831336): §4 наблюдение — сравнения с NaN ложны → молчаливый pass на вырожденных промежуточных; защищено предшествующими фильтрами; 29 выживших arc_block подсчитаны точно, паттерн системен в 4 канонических поверхностях; is_finite-гейт — кандидат v0.3.x.
- **emission-гейт emit_r_arcs** (agent E, 200e53c, находка D): true вне fanuc/haas → отказ валидации («тихое игнорирование запрещено»); честный текст M02 (старый факт-неверен, прибит тестом). Анализ применимости:wcs-map и program_end_style неприменимы/не нужны на grbl/linuxcnc (посылка G59.x-grbl опровергнута — grblHAL-дельта).
- **COVERAGE v0.9.6**: §6.71 закрыт, CHANGELOG emission-гейт. Workspace **1115/0**.

## 11. Итерация 11 — решения ревью + референс-верификация 3-axis границы — ЗАВЕРШЕНА

*(итерация 11: NURBS 11/15 Ok+Raw — корень Err: строки-продолжения G5.x; Q10 закрыт навсегда; NaN-классификация; корпус-аудит 157 — 131 чисто 3-осевых, SmilinJack 4-осевой пойман до публикации; workspace был 1123/0)*

**Ревью-триада выполнена** (решения пользователя 2026-11-22):
1. **NURBS/Q10 закрыты** (SPEC v0.19, 38f583c; реализация D, 449cd42): Q10 — инлайнинг отклонён навсегда для L1 (пин «тело ровно 1 раз»); NURBS — найден **корень** Err (строки-продолжения G5.x без G-кода парсились как модальные ходы) → spline_active-режим, **11/15 файлов Ok+Raw** (4 Curva честно Err на `#`-параметрах — класс §1.1, не маскирован); свип 65/92→76/81, R3 держатся.
2. **NaN-swallow классифицирован**: известное ограничение, не блокер (§4: лексер физически не производит NaN, арифметика ограничена; is_finite-гейт — кандидат v0.3.x).
3. **Формулировки**: §1 N+2 эмпирический claim + честная временная формулировка; **§6.71 закрыт** (129e639) + emission-гейт emit_r_arcs (200e53c, находка D); **корпус-аудит 157** (9391b85): 131 чисто 3-осевых (83.4%), 26 именованных скрытых классов; named evidence переписан в три раздельных claim'а с новыми пробами (bricka 49-Z fixed-point, shpk кросс байт-идентичен).

**Workspace: 1183/0.** COVERAGE v0.12, SPEC v0.20 (§2.5-ревизия v0.21).

### 11.1 Итерация 12 — волна 1 (v0.20) + волна 2 (Macro-B/chain-continuity/units) — ЗАВЕРШЕНА

Волна 1 (SPEC v0.20, зонды 1–8, COVERAGE v0.11): G92-mid/bare-T/end-not-last во всех 4 адаптерах, ERR-инвентаризация 81 класса (потенциал 48/81), workspace был 1161/0.

Волна 2 (закрытие итерации 12):
- **linuxcnc** (13e3e13; тесты 174→182): **Macro-B line-level Raw** (`#`-параметры/`[expr]`/IF/GOTO/THEN/O-слова — зеркало fanuc-политики v0.16; 38+ файлов Err→Ok+Raw; Curva dead-assignment = Raw не drop); **units-switch mid-program легален** (SPEC §2.5-принцип 5 ревизия v0.21 — Raw-носитель + конверсия 25.4 в пределах допуска; **anchor-баг-находка+фикс**: convert_units_anchor — хранимый end последнего motion-опа конвертируется синхронно с last_motion.end на смене системы (дуга-якорь) — N7 exit=0); G2.2/G3.2 NURBS-открывашки → spline_active-семья; `M1.`-трейлинг-точка; `G4 X`-dwell legacy; E-слово легально только в G76.
- **grbl** (fc01bb9 + db894d6; тесты 138→145): **chain-continuity через Raw-блоки** (Raw с осями → chain-break unknown-start: ложные «невозможная дуга/radius mismatch» устранены на 5 файлах → честные RUNTIME_STATE); **protocol-strip** `c<=' '` вне комментариев (находка control-байта \x01 в mit_cws_fresa; зонд-конвергенция с rs274); units-switch (Err «one unit system» снят — convert_units_anchor + UnitsInverse, независимая реализация сошлась с D); ARC_OFFSETS_MISSING текст побайтно зеркален linuxcnc; **единственный Ok→Err флип = RTCP-файл** (бесчестный chaining сквозь rotary → честный partial-end; RTCP = этап 12).
- **Свип Ok 81→116/157** (артефакты 43ad7c9); harness класс-листы N7 → line-class 30/30 (6a6e3cb); manifest N7 ревизия v0.21 (e4520da).

## 12. Итерация 12 — «исправь недоразумение» (G92-mid / bare-T / end-not-last) — ЗАВЕРШЕНА

**Итерация 12 закрыта целиком** (волна 1 SPEC v0.20: G92-mid/bare-T/end-not-last во всех 4 адаптерах по зонд-фактам rs274+grbl-source; волна 2: Macro-B-linuxcnc / chain-continuity / units-switch v0.21 — свип Ok 116/157, workspace **1183/0**, COVERAGE v0.12; ревью-очередь пуста). Единственный Ok→Err флип волны 2 — RTCP-файл: честный partial-end вместо бесчестного chaining'а сквозь rotary (RTCP = этап 12).

## 13. ЭТАП 12 — L2 (параметрика, трансформации, 4/5-осей): фазы 12.1 + 12.2 + 12.3-кодирование — ЗАВЕРШЕНЫ

Дизайн-док `EID-L2-DESIGN.md` v0.1 (d3fd57e — материализован в репо после инцидента «док в чате, не в дереве»; агент A начал фундамент по встроенной в брифе спецификации — она совпала с доком по существу). **Ключевой принцип дока «оси в AST, переменные в lowering первыми»** — 5-осевые параметрические кадры (`G0 A#100`) парсятся в полный L2 AST, а частичный lowering даёт Raw с подставленными значениями — T4 дословно.

### 13.1 Фаза 12.1 — L2 AST + парсеры 3 диалектов

- **Фундамент** (agent A, 22ddb0a + синхронизация с доком): AST §2 дословно (Expr/Val::Null/SysVar/Axis/ExprPose/TransformKind/L2Stmt с SrcSpan), eval #0-семантика — 22 теста T3 (арифметика Null→0, div/LN на Null → E_INVALID_INPUT, EQ Null-тождество vs GT/GE→0.0, AND/OR/XOR i64, присваивание копирует Null, адресное слово с Null игнорируется), interp D0–D2 скелет, T5-бюджеты. 51 тест.
- **Парсеры** (agents B2/D2): **parser_macro_common** (~1250 строк: лексер слитных слов/#[expr]/скобок, курсорный expr-парсер с приоритетами Fanuc — подтверждено идиомой Renishaw «#170 AND 4 EQ 4», DO/END-стек 1..3, O-sub/M97-carving) + **parser_fanuc/haas** (G65/G66-args вкл. M-слово, M98-слип P52000=5×O2000, #4001-4030 ModalGroup; M97 NLabel+L-expr, G101-Mirror, #500–699/#109xxx VendorSpecific, bare-IF→IfThen) — **19/19 целевых файлов Ok**; **parser_linuxcnc** (O-word sub/while/if/call, named #<_var>, EXISTS/ATAN2, рекурсия ×3, не-§2.4 → целостные Op::Raw-носители verbatim) — **34/34 Ok**.
- **Амендменты дизайн-дока**: v0.1.1 UnaryOp::Exists + BinaryOp::Atan2 (корпус-ценз), v0.1.2 SubTarget::Named (Copy снят), v0.20.1 (от A) Label/Return/labels-таблица.

### 13.2 Фаза 12.2 — L2Evaluator D0–D3 + Pass-1 lowering (agent A, e48e266)

D0 pose_resolve (Null = слово отсутствует) · D1 стек ×9 (Null-аргумент проброс, M99→Return) · D2 CFG IfGoto label-таблицы + per-loop бюджет · D3 мёртвые ветки (Sys-чтение в мёртвой IF-ветке не даёт ложного E_RUNTIME_STATE) · SysVar-маппинг (ModalGroup снапшот / WcsOffset→Raw{TABLE} / позиции→честный останов) · аффинные Rotate2D/Scale + Mirror дефолт-отказ / allow_mirror_bake→Cw↔Ccw+Notice · дуги → Op::Arc канон.

**Trace-equivalence DoD (docker rs274): semantic_diff == 0** — factorial-рекурсия (глубина 5), flowsnake corpus (**3072 ходов, ×1026 рекурсивных вызовов** — 4dp-квантование пережило рекурсию без накопления), Null-аргумент G65. T4 дословно (payload «G0 X15.0000 A15.0000»), T5/T6/T1 конфирм.

### 13.3 Фаза 12.3 — SMT-кодирование (agent E, 86f6894)

Term-IR Formula/NumExpr/ValTerm + Encoder по таблице §3.2 дословно. **Кросс-валидация 444 кейса с eval.rs — 0 расхождений** (44 рукописных + 400 LCG-генерируемых; генератор сам поймал modeling-фикс: допуск Macro B 1e-9 раскрыт структурно в формулу — бит-в-бит). Solver-trait за feature="smt"; term-builder НЕ гейтится — кросс-валидация на каждой сборке. Подключение Z3/CVC5 — subprocess путь (T2 std-only сохраняется).

### 13.4 T1–T6 чек-лист: ВСЕ КОНФИРМ

T1 L1-passthrough 1:1 · T2 std-only (+фича только для будущего solver) · T3 #0-семантика 22 теста · T4 частичный lowering дословно · T5 бесконечный WHILE → E_LIMIT_EXCEEDED (3 конфигурации) · T6 Mirror дефолт-отказ / allow_mirror_bake→инверсия+Notice.

### 13.5 Остатки этапа 12 (честно)

12.4 L2↔L2 emit — не начат · 12.5 кинематика — future (нужен 5-осевой симулятор-оракул) · trace-equivalence пока на linuxcnc-пути (fanuc-MB парсеры долетели к концу — зеркальная сверка следующей) · endsub-retval/named-scoping/polar-@^ — вне §2.4, L1Op-носители · G67/G17-18-19 AST контракты — parser-фидбек D2/A · корпус: WithTransform ×0 (трансформации без корпусного драйвера — синтетика) · l2_available O-word маркеры применены (59006fa).

## 14. Итерация 14 — остатки перспективы — ЗАВЕРШЕНЫ

- **NaN is_finite-гейт v0.3.x применён** (agent B, verify.rs): ε_pos non-finite → program-refusal, resid non-finite → refusal громко, linking |Δ| non-finite → refusal; v03_gate_* тесты. EQUIVALENCE §4 note: ограничение снято гейтом.
- **fanuc trace без шима**: while_clear переписан — зонд rs274 показал, что LinuxCNC **нативно исполняет** косвенное присваивание `#[1]=0` (X вернул 0) — Y-шим убран, semantic_diff 0 без него; docker-образ пересобран после потери при перезапуске (taskkill снёс WSL-данные — диагностировано и починено).
- **Docker-конфликт с параллельным trading-bot контейнером**: 300с-таймауты харденинга 18a743e пережили конфликт — честный отказ вместо зависания.

**Числа:** **1467 passed / 0 failed** (1183 → 1467: +284 от NaN-гейта, без-шим trace, волны-2).

## 15. Дальше (этап 12 фаза 12.4+ + отложенное)

Фаза 12.4 (L2↔L2 emit) при потребности · fanuc-MB trace-equivalence (зеркальная сверка) · emission-профили grblHAL/FluidNC при адаптерах · NaN-is_finite-гейт v0.3.x · Haas `?`-остатки (Setting 27 домен, SpaceInWord, L0) — manual-сверка при доступе к станку · intra-file парсинг (Амдал-хвост R10) · **Ревью-очередь пуста** — все решения ревью реализованы (NURBS/Q10 v0.19, NaN-классификация, формулировки).
