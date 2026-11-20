# COVERAGE — матрица диспозиций кодов

**Версия:** v0.5 (эмиссия дуг: in-plane пары; реестр +12), привязана к SPEC-EID-L1 **v0.9**, EQUIVALENCE **v0.2**, CORPUS-CONTRACT **v0.4**, grammar.ebnf **v0.1**
**Правило приёмки:** каждый документированный код GRBL 1.1 / grblHAL / FluidNC / LinuxCNC имеет ровно одну диспозицию на пересечении с диалектом. 100% словаря — критерий этапа 0 (SPEC §15.1).

---

## 0. Правила матрицы

1. **Колонки** — адаптер *источника*: что делает парсер этого диалекта, встретив код. Ограничения *эмиссии* (узел → цель) — отдельная таблица §5.
2. **Диспозиция** описывает поведение транслятора, не «правильность» кода. `RAW(code)` = блок сохраняется в EID как raw с этим кодом: same-dialect round-trip сохраняет байт-в-байт, кросс-диалект — отказ `E_RAW_UNTRANSLATABLE` (SPEC §8).
3. **Предположение самодостаточности файла (уточнено, EQUIVALENCE §1.2):** модальные режимы на входе = дефолты контроллера; программа не полагается на состояние из предыдущего файла. Нарушение — `E_RUNTIME_STATE`. **WCS origins — не модальное состояние**, а внешний контракт оператора: физические значения origin вне программы; эквивалентность геометрии — условная (идентичная настройка таблиц на источнике и цели). Machine-frame ходы — локальное исключение: origin обязан быть в тексте (SPEC §5.3).
4. **`?`** = поведение заявлено, но не сверено по source-коду контроллера. Реестр — §6. До снятия `?` строка не считается финальной.
5. FluidNC: словарь наследует GRBL 1.1; дельты помечены `?` до сверки с FluidNC wiki.

## 1. Таксономия

### 1.1 Коды диагностики (стабильны между версиями; полные определения — SPEC §8)

| Код | Класс | Смысл |
|---|---|---|
| `E_INVALID_INPUT` | вход сломан | невозможная дуга, lexical-мусор, «голые» координаты без motion-кода |
| `E_UNSUPPORTED_SYNTAX` | возможности | код/конструкция вне словаря L1 или вне словаря цели |
| `E_UNSUPPORTED_SEMANTICS` | возможности | синтаксис есть, семантика не представима ходами |
| `E_REQUIRES_MACHINE_TABLE` | возможности | значение зависит от таблицы станка (home, TLO по H, таблицы инструментов) |
| `E_RUNTIME_STATE` | возможности | значение зависит от состояния исполнения (параметры, block delete, ожидание входа) |
| `E_LIMIT_EXCEEDED` | guard транслятора | порог ресурсов; см. §1.2 |
| `E_RAW_UNTRANSLATABLE` | контракт | кросс-диалект при наличии RAW в программе |

### 1.2 Пороги `E_LIMIT_EXCEEDED` (v0.0, фиксированные; переопределяются профилем с этапа 8)

| Порог | Дефолт | Защищает от |
|---|---|---|
| `max_ops_per_program` | 200 000 | гигантские развёртки / сгенерированный мусор |
| `max_unroll_iterations` | 10 000 (сумма L/K на блок цикла) | blowup G83×L9999 |
| `max_source_lines` | 1 000 000 | файлы-аномалии |
| `max_word_chars` | 32 | мусорные слова, зависание лексера |

Поля диагностики: код, имя порога, порог, фактическое значение, source_span.

### 1.3 Диспозиции (словарь матрицы)

| Метка | Значение |
|---|---|
| `NODE(n)` | узел/поле SPEC §5 |
| `ABSORB` | разрешается в поля опов или метаданные без отдельного узла (модальные режимы) |
| `META` | метаданные программы (path_mode, units) |
| `IGNORED_SAFE` | игнорируется безопасно; **узкий класс**: лексика/контроль без process-значения (N-слова, `%`, G80). Определение — SPEC §8; расширение — только per-code здесь, с обоснованием |
| `RAW(code)` | сохраняется как raw с кодом из §1.1 |
| `N/A` | код отсутствует в словаре диалекта (нечего парсить) |

**Правило семейств (v0.0):** cancel-коды семейства наследуют диспозицию активных кодов семейства *в этом файле*: если в файле есть `G43`/`G43.1` (raw), то `G49` — тоже `RAW`; если активных кодов нет — `IGNORED_SAFE`. Иначе same-dialect round-trip оставляет компенсацию активной навсегда (G43.1 сохранён, G49 выброшен → поведение эмиссии ≠ источник). Правило детерминировано: сканер отмечает семейство целиком до диспозиций.
**Состав семейств (v0.1, выписан явно):** семейство G40 = {G41, G41.1, G42, G42.1}; семейство G49 = {G43, G43.1, G43.2}.

## 2. Матрица G-кодов

### 2.1 Движение

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | L1 | Примечание |
|---|---|---|---|---|---|---|
| G0 | NODE(rapid) | = | = | NODE(rapid) | rapid | |
| G1 | NODE(linear) | = | = | NODE(linear) | linear | |
| G2/G3 | NODE(arc) | = | = | NODE(arc) | arc | R и IJK → канон §6; helical — в обоих; G90.1/G91.1 → ABSORB |
| G5 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | кубич. B-spline (XY); **scope decision**: декомпозируем статически в linear, но меняет характер движения — canonical decomposition кандидат этапа 12 |
| G5.1 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | квадр. B-spline; см. G5 — scope decision |
| G5.2/G5.3 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | NURBS (experimental); см. G5 — scope decision |
| G33 / G33.1 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | spindle-synced движение |
| G38.2/.3/.4/.5 | RAW(E_UNSUPPORTED_SEMANTICS) | = | = | RAW(E_UNSUPPORTED_SEMANTICS) | — | пробинг — этап 12 |
| G80 | IGNORED_SAFE | = | = | IGNORED_SAFE | — | отмена cycle-моды; циклы в L1 немодальны. «Голая» координатная строка после G80 без motion-кода — `E_INVALID_INPUT` |

### 2.2 Модальные режимы → ABSORB

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | L1 | Примечание |
|---|---|---|---|---|---|---|
| G17/G18/G19 | ABSORB (плоскость парсера) | = | = | ABSORB | — | валидация нормали дуги, §6 |
| G20/G21 | ABSORB (units программы) | = | = | ABSORB | META(units) | переключение в середине → `E_UNSUPPORTED_SEMANTICS` (§3) |
| G90/G91 | ABSORB (абсолютные координаты) | = | = | ABSORB | — | |
| G90.1/G91.1 | N/A (только G91.1) | = | = | ABSORB | — | центр дуги → канон §6 |
| G93/G94 | ABSORB (feed-разрешение §4) | = | = | ABSORB | state.feed | |
| G61 | META(exact_path) `?` | `?` | `?` | META(exact_path) | META | GRBL: код в словаре README — фактическое поведение сверить по source (§6.1) |
| G61.1 | N/A | `?` | `?` | META(exact_stop) | META | |
| G64 [P- Q-] | N/A | `?` | `?` | META(blend(t, q)) | META | Q = Naive CAM Detector — в метаданные не входит, теряется с warning'ом |

### 2.3 Системы координат и фреймы

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | L1 | Примечание |
|---|---|---|---|---|---|---|
| G54–G59 | ABSORB (wcs) | = | = | ABSORB | state.wcs | |
| G59.1/.2/.3 | N/A (нет в словаре) | `?` | `?` | ABSORB (wcs) | state.wcs | эмиссия в GRBL невозможна — §5 |
| G54.1 Pn | N/A | N/A | N/A | N/A | — | **отсутствует в словаре обоих MVP-диалектов** (официальная документация LinuxCNC: только G54–G59.3); Fanuc/HAAS-стиль — этап 10 |
| G10 L2 | NODE(wcs_define) | = | = | NODE(wcs_define) | wcs_define | разрешён в любой позиции (§5.3) |
| G10 L20 | NODE(wcs_set_here) | = | = | NODE(wcs_set_here) | wcs_set_here | только пресетка до первого motion-опа |
| G10 L1/L10/L11/L28/… | N/A | `?` | `?` | RAW(E_REQUIRES_MACHINE_TABLE) | — | записи таблиц инструментов / прочие |
| G92 | META(frame_preset) | = | = | META(frame_preset) | frame_preset | только пресетка; координаты, присваиваемые текущей позиции (§3) |
| G92.1 | RAW(E_UNSUPPORTED_SEMANTICS) | = | = | RAW(E_UNSUPPORTED_SEMANTICS) | — | Q4 |
| G92.2/G92.3 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | Q4 |
| G53 | NODE-поле `frame=machine` | = | = | NODE-поле | state.frame | правило machine-frame §5.3 |
| G28 / G30 | RAW(E_REQUIRES_MACHINE_TABLE) | = | = | RAW(E_REQUIRES_MACHINE_TABLE) | — | Q1; узел — этап 8 |
| G28.1 / G30.1 | RAW(E_REQUIRES_MACHINE_TABLE) | = | = | RAW(E_REQUIRES_MACHINE_TABLE) | — | запись home из текущей позиции — тот же класс, что Q1 |

### 2.4 Компенсации

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | L1 | Примечание |
|---|---|---|---|---|---|---|
| G40 | IGNORED_SAFE* | = | = | IGNORED_SAFE* | — | *правило семейств §1.3 |
| G41/G42/G41.1/G42.1 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | применение компенсации геометрией — отдельная фича (не-цель §1.2) |
| G43 H | N/A | RAW(E_REQUIRES_MACHINE_TABLE) `?` | `?` | RAW(E_REQUIRES_MACHINE_TABLE) | — | TLO из таблицы |
| G43.1 | RAW(E_UNSUPPORTED_SEMANTICS) | RAW(E_UNSUPPORTED_SEMANTICS) | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | динамический TLO; **кандидат ревизии**: значение в тексте → разрешаемое поле `tlo` (Z-сдвиг). Триггер — **первое появление в Tier B**, не частотность (ревью §7.6) |
| G43.2 | N/A | RAW(E_REQUIRES_MACHINE_TABLE) `?` | `?` | RAW(E_REQUIRES_MACHINE_TABLE) `?` | — | дополнительный TLO; семантика (табличная vs динамическая) — сверить, §6; входит в семейство G49 (B3) |
| G49 | IGNORED_SAFE* | = | = | IGNORED_SAFE* | — | *правило семейств §1.3 |

### 2.5 Циклы (LinuxCNC; GRBL-семейство — N/A)

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | L1 | Примечание |
|---|---|---|---|---|---|---|
| G81/G82/G83/G85/G86/G89 | N/A | N/A | N/A | разворачиваются (§7 SPEC) | rapid/linear/dwell | G86 — рестарт шпинделя обязателен (Q9 закрыт) |
| G73 | N/A | N/A | N/A | разворачивается | rapid/linear | chip-break: отвод над последней глубиной |
| G84/G74 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | нарезание резьбы, синхронизация |
| G76 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | LinuxCNC G76 = **токарный цикл резьбы** (ZX, синхронизация) → вне скоупа §1.2. Fanuc G76 (fine boring) — этап 10 |
| G87/G88 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | LinuxCNC: «accepted, behavior undefined» |
| G98/G99 | N/A | N/A | N/A | ABSORB (правило §7.6) | — | initial Z требует определённой позиции, иначе `E_RUNTIME_STATE` |

### 2.6 Прочее

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | L1 | Примечание |
|---|---|---|---|---|---|---|
| G4 | NODE(dwell) | = | = | NODE(dwell) | dwell | P секунды в GRBL и LinuxCNC — подтверждено (grbl interface.md; LinuxCNC gcode.html) |
| G96/G97 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | CSS — токарное, вне скоупа §1.2 |
| G7/G8 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | диаметральный режим — токарное |
| G50/G51 | N/A | N/A | N/A | N/A `?` | — | scaling: в стабильной документации LinuxCNC отсутствует, legacy 2.4–2.5 содержал — сверить (§6); Fanuc/HAAS — этап 10 |

## 3. Матрица M-кодов

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | L1 | Примечание |
|---|---|---|---|---|---|---|
| M0 | NODE(program_stop) | = | = | NODE(program_stop) | program_stop | optional=false |
| M1 | NODE(program_stop) | = | = | NODE(program_stop) | program_stop | optional=true; Q8 |
| M2 / M30 | NODE(program_end) | = | = | NODE(program_end) | program_end | различие M2/M30 — поведение контроллера после конца, вне L1 |
| M3/M4/M5 | ABSORB (spindle) | = | = | ABSORB | state.spindle | синтаксис `$n` (LinuxCNC multi-spindle): `$n` ≠ default → `E_UNSUPPORTED_SEMANTICS` (multi-spindle вне скоупа v0.0) |
| M6 | N/A → RAW(E_UNSUPPORTED_SYNTAX) | NODE(tool_change) | NODE(tool_change) `?` | NODE(tool_change) | tool_change | LinuxCNC M6 не меняет TLO (G43 отдельно) — совместимо с узлом |
| M7/M8/M9 | ABSORB (coolant) | = | = | ABSORB | state.coolant | |
| M19 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | ориентация (R/Q/P), нужен энкодер; Fanuc G76 зависит — этап 10 |
| M48/M49 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | override — process intent (условия исполнения, напр. disable перед растачиванием); same-dialect fidelity требует сохранения (ревью §7.4) |
| M50/M51 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | тот же класс, что M48/M49 |
| M52/M53 | N/A | `?` | `?` | RAW(E_RUNTIME_STATE) | — | adaptive feed = вход HAL в рантайме; feed-hold switch |
| M60 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | pallet change + pause |
| M61 | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | RAW(E_UNSUPPORTED_SEMANTICS) `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | номер без физической смены не соответствует семантике `state.tool` (физический, SPEC §4); ABSORB «врал» бы о физике. Кандидат ревизии — поле `selected_tool` (ревью §7.5) |
| M62–M65 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | цифровые выходы |
| M66 | N/A | `?` | `?` | RAW(E_RUNTIME_STATE) | — | ожидание входа |
| M67/M68 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | аналоговые выходы |
| M70–M73 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | стек модального состояния; в L1 нет состояния-потока; кандидата до L2 нет |
| M98/M99 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SYNTAX) | — | Fanuc-style subs; **Q10**: статический инлайнинг без параметров — кандидат ревизии по Tier B |
| M100–M199 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | внешние executables — вне вопроса |
| T-слово | ABSORB (state.tool, только в паре с M6; одиночное T — `E_INVALID_INPUT`) | `?` — семантика T-без-M6 при наличии M6-семейства, §6.8 | `?` — §6.8 | ABSORB (T без M6 — **предвыбор**: RAW(E_UNSUPPORTED_SEMANTICS)) | state.tool | одиночное T в LinuxCNC легально (предвыбор) — не E_INVALID_INPUT (исправлено ревью); кандидат ревизии `selected_tool` |

## 4. Прочий словарь

| Элемент | Диспозиция | Примечание |
|---|---|---|
| N-слова | IGNORED_SAFE ×все | номера кадров |
| `%` маркеры программы | IGNORED_SAFE ×все | |
| Комментарии `()` и `;` | аннотации (SPEC §11) | не в матрице отказов |
| `/` block delete | LinuxCNC: RAW(E_RUNTIME_STATE); GRBL: N/A `?` (§6) | исполнение зависит от переключателя станка — не статично |
| `$`-строки (GRBL config) | RAW(E_UNSUPPORTED_SYNTAX) | конфигурация станка, не программа |
| Слова A/B/C/U/V/W | RAW(E_UNSUPPORTED_SEMANTICS) ×все | ротационные и линейные 4-е оси — этап 12 (UVW добавлены ревью) |

## 5. Эмиссия: ограничения целей (узел → диалект цели)

**Правило класса отказа при эмиссии (v0.1, ревью B2):** конструкция отсутствует в словаре цели → `E_UNSUPPORTED_SYNTAX`; присутствует, но не представима ходами → `E_UNSUPPORTED_SEMANTICS`.

| Узел/мета | → GRBL 1.1 | → grblHAL | → FluidNC | → LinuxCNC |
|---|---|---|---|---|
| tool_change | отказ `E_UNSUPPORTED_SYNTAX` (M6 вне словаря GRBL) | M6 | M6 `?` | M6 |
| state.wcs = G59.1/.2/.3 | отказ `E_UNSUPPORTED_SYNTAX` | `?` | `?` | ок |
| path_mode exact_path | G61 `?` + warning (поведение зависит от junction-настроек) | `?` | `?` | G61 |
| path_mode exact_stop | warning: кода нет; точная остановка недостижима | `?` | `?` | G61.1 |
| path_mode blend(t) | warning: G64 нет; GRBL смешивает по своим настройкам | `?` | `?` | G64 P- |
| dwell | G4 P- (сек) | = | = | G4 P- |
| frame=machine | G53 | = | = | G53 |
| frame_preset / wcs_define / wcs_set_here | G92 / G10 L2 / G10 L20 | = | = | те же |
| arc (включая helical) | G2/G3 (IJK инкрементальный — в GRBL **не режим, а единственный способ**; G90.1 вне словаря); эмитятся **только in-plane offset-пары** (G17: I+J, G18: I+K, G19: J+K) — I/J/K literal X/Y/Z в обоих диалектах (§6.11), перестановка не требуется; unused offset в GRBL не ошибка (gcode.c: cleared, не FAIL). **Полное кольцо (sweep=360°, pitch=0) эмитится БЕЗ end-слов** (`G2 I.. J..` — легальный GRBL-формат): слова end==start при перепарсе квантуются в старт и ошибочно триггерили бы zero-length drop (агент C, итерация 3; винтовой полный оборот pitch≠0 эмитится с end-словом оси) | = | = | G2/G3 (режим G90.1/G91.1 → ABSORB; эмиссия — G91.1 с явной установкой) |
| циклы (развёрнутые ходы) | само собой | = | = | развёрнутый вывод — документированный verbose (SPEC §1.2) |

## 6. Реестр `?` (сверить по source-коду / вики на этапе 3)

1. **GRBL G61** — код в словаре README; фактическое поведение (junction settings vs exact path) — по source `gcode.c`.
2. **grblHAL дельты словаря**: G59.1–.3, G61/G61.1/G64, M48/M49, G54.1, G10 L1/…, G92.2/.3 — wiki неполный, сверить.
3. **GRBL block delete «/»** — поддерживается ли неявно.
4. **FluidNC дельты** — M6 (заявлен), G43.1, G59.x, словарь в целом — FluidNC wiki «Supported Gcodes».
5. **GRBL G4 «P > 0 строго»** — подтверждено interface.md (в реестре для полноты).
6. **grblHAL/FluidNC G43.2** — семантика (табличная vs динамическая): классификация REQUIRES vs USEMANTICS (B3).
7. **LinuxCNC G50/G51** — подтвердить отсутствие в стабильном словаре (legacy 2.4–2.5 содержал scaling).
8. **grblHAL / FluidNC: одиночное T без M6** — является ли предвыбором (как в LinuxCNC) при наличии M6-семейства; в GRBL 1.1 M6 нет вовсе, наследование `=` не обосновано.
9. **R + вне-плоскостное слово — решено ✔ (оба диалекта).** LinuxCNC: helical arc в R-форме документирован («G17 G2 X10 Y15 R20 Z5», g-code.html). GRBL: **source-check gcode.c/master, 2026-11-20** — R-математика вычисляет центр только в плоскости (`ijk[axis_0/1]`), вне-плоскостное слово проходит в `mc_arc` нетронутым → helix; error-path для R+off-plane отсутствует. Следствие: вектор #46 — GRBL-файл материализован.
10. **Повторный tool_change с тем же номером** — legal no-op vs переустановка: зависит от контроллера; отложено (обсуждение CORPUS v0.2).
11. **GRBL G17/G18/G19 — решено ✔** (source-check gcode.c/master, 2026-11-20): `case 17: case 18: case 19:` — все три плоскости парсятся; assignment `PLANE_SELECT_XY/ZX/YZ` даёт полные тройки `axis_0/axis_1/axis_linear`; error-path для G18/G19 отсутствует. I/J/K — буквально X/Y/Z в GRBL и LinuxCNC (пару in-plane осей выбирает плоскость, ремапа нет). **Следствие: гипотеза ревью «GRBL = G17 only» опровергнута source'ом; векторы #14/#15/#16 — R4-отказов для GRBL не имеют; эмиссионное правило in-plane пар — §5.**
12. **grblHAL G17/G18/G19 + IJK mapping — решено ✔** (source-check grblHAL/core gcode.c/master, 2026-11-20): `gc_get_plane_data()` — все три плоскости, отказа нет; `#define I_VALUE X_AXIS` — I/J/K literal, ремапа нет. **Наследование GRBL 1.1 (§6.11) подтверждено source'ом** — колонки `=` в §2.1 теперь обоснованы.

## 7. Спорные строки для ревью (первым делом)

1. **G86 restart — разрешён документацией LinuxCNC**: feed на дно → dwell P сек → шпиндель off → rapid на clear Z → «restart the spindle in the direction it was going». Разворачивание восстанавливает `state(spindle)` (направление + обороты). Q9 закрыт, SPEC §7 обновлён.
2. **G76 — переопределён**: LinuxCNC G76 = токарный цикл резьбы (ZX-плоскость, синхронизация шпинделя) → отказ вне скоупа §1.2. Fanuc G76 (fine boring w/ shift, M19) — этап 10. Q6 SPEC переформулирован.
3. **G4 — единицы подтверждены**: секунды в GRBL (interface.md: «P is in seconds, must be > 0.0») и LinuxCNC (P float seconds).
4. **M48/M49/M50/M51 → RAW(E_UNSUPPORTED_SEMANTICS)** — принято ревью: override = process intent; IGNORED_SAFE противоречил бы SPEC §8. Класс IGNORED_SAFE сужен до лексики (N, %, G80), определение добавлено в SPEC §8.
5. **M61 → RAW(E_UNSUPPORTED_SEMANTICS)** — принято ревью: `state.tool` определён как **физический** инструмент (SPEC §4); ABSORB искажал бы физику. Заодно найдено: одиночное T в LinuxCNC — легальный предвыбор, был ошибочно `E_INVALID_INPUT` → теперь RAW. Кандидат ревизии — поле `selected_tool`.
6. **G43.1 = RAW + ревизия `tlo` при первом появлении в Tier B** — не «по частотности» (ревью §7.6).
7. **G40/G49 — правило семейств** (§1.3), состав выписан явно: G40 = {G41, G41.1, G42, G42.1}; G49 = {G43, G43.1, G43.2}.
8. **tool_change → GRBL — класс исправлен на `E_UNSUPPORTED_SYNTAX`** (B2: M6 вне словаря цели; правило §5). G59.1–.3 → GRBL — SYNTAX, было корректно.
9. **M98/M99 — классы SPEC↔COVERAGE согласованы** (B1): SPEC §1.1 разделён — control flow/subs/макросы → `E_UNSUPPORTED_SYNTAX`; `#`-переменные/system vars/block delete → `E_RUNTIME_STATE`.
8. **G54.1 Pn — отсутствует в словаре обоих MVP-диалектов** (уточнение к твоему чек-листу: это не LinuxCNC-код; официальная документация LinuxCNC знает только G54–G59.3). Fanuc/HAAS — этап 10.

## 8. Источники

- [Grbl 1.1 README (словарь кодов)](https://github.com/gnea/grbl/blob/master/README.md)
- [Grbl interface.md (G4 P секунды, слова)](https://github.com/gnea/grbl/blob/master/doc/markdown/interface.md)
- [Grbl v1.1 Commands wiki](https://github.com/gnea/grbl/wiki/Grbl-v1.1-Commands)
- [grblHAL core wiki (G43*/G43.1/G43.2/G49, M6, M61)](https://github.com/grblHAL/core)
- [LinuxCNC G-code overview](https://linuxcnc.org/docs/stable/html/gcode/g-code.html)
- [LinuxCNC M-code overview](https://linuxcnc.org/docs/stable/html/gcode/m-code.html)
- [DryRun CNC — GRBL 1.1 / grblHAL supported G-codes](https://dryruncnc.com/docs/grbl)
- [FluidNC wiki — supported G-codes](https://wiki.fluidnc.com/en/stable/features/paths) (дельты, `?`)
