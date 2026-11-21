# COVERAGE — матрица диспозиций кодов

**Версия:** v0.8.1 (реестр +2: rulings ревью этапа-1 eid-fanuc — F-DPI исключение, хвост после PEND), привязана к SPEC-EID-L1 **v0.16**, EQUIVALENCE **v0.3.1**, CORPUS-CONTRACT **v0.7**, grammar.ebnf **v0.3**; Fanuc-колонка — из `fanuc_dialect_analysis.md` v0.0 (research; `?`-строки — несверено, реестр §6)
**Правило приёмки:** каждый документированный код GRBL 1.1 / grblHAL / FluidNC / LinuxCNC имеет ровно одну диспозицию на пересечении с диалектом. 100% словаря — критерий этапа 0 (SPEC §15.1).

---

## 0. Правила матрицы

1. **Колонки** — адаптер *источника*: что делает парсер этого диалекта, встретив код. Ограничения *эмиссии* (узел → цель) — отдельная таблица §5.
2. **Диспозиция** описывает поведение транслятора, не «правильность» кода. `RAW(code)` = блок сохраняется в EID как raw с этим кодом: same-dialect round-trip сохраняет байт-в-байт, кросс-диалект — отказ `E_RAW_UNTRANSLATABLE` (SPEC §8).
3. **Предположение самодостаточности файла (уточнено, EQUIVALENCE §1.2):** модальные режимы на входе = дефолты контроллера; программа не полагается на состояние из предыдущего файла. Нарушение — `E_RUNTIME_STATE`. **WCS origins — не модальное состояние**, а внешний контракт оператора: физические значения origin вне программы; эквивалентность геометрии — условная (идентичная настройка таблиц на источнике и цели). Machine-frame ходы — локальное исключение: origin обязан быть в тексте (SPEC §5.3).
4. **`?`** = поведение заявлено, но не сверено по source-коду контроллера. Реестр — §6. До снятия `?` строка не считается финальной.
5. FluidNC: словарь наследует GRBL 1.1; дельты помечены `?` до сверки с FluidNC wiki.
6. **Fanuc (v0.8):** колонка = mill-профиль (0i-D/F, 16i/18i/21i, 30i/31i/32i; «полный» 30i-профиль с paid-опциями — отчёт §0.3); валидация по конкретному станку — профиль этапа 8. **`=` в Fanuc-колонке = та же диспозиция, что LinuxCNC** (ближайший диалект; отличается от смысла `=` в grblHAL/FluidNC-колонках, где `=` наследует GRBL) — общие строки не дублируются. Источник — `fanuc_dialect_analysis.md` v0.0; `?` в Fanuc-ячейке = несверено по source/мануалу конкретной серии — реестр §6, до снятия строка не финальная и не приёмка.

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
| `ERR(code)` | **жёсткий отказ** (не raw; v0.8, Fanuc-колонка): конструкция, чьё raw-сохранение молча исказило бы интерпретацию **последующих** опов (смена фрейма mid-program, смена единиц, структура программы) — класс «структурный → Err» правила двух классов (SPEC §8 правило 5; CORPUS §1.2). Не входит в RAW-домен R1-text; same-dialect — отказ с диагностикой, не сохранение |
| `N/A` | код отсутствует в словаре диалекта (нечего парсить) |

**Правило семейств (v0.0):** cancel-коды семейства наследуют диспозицию активных кодов семейства *в этом файле*: если в файле есть `G43`/`G43.1` (raw), то `G49` — тоже `RAW`; если активных кодов нет — `IGNORED_SAFE`. Иначе same-dialect round-trip оставляет компенсацию активной навсегда (G43.1 сохранён, G49 выброшен → поведение эмиссии ≠ источник). Правило детерминировано: сканер отмечает семейство целиком до диспозиций.
**Состав семейств (v0.1, выписан явно):** семейство G40 = {G41, G41.1, G42, G42.1}; семейство G49 = {G43, G43.1, G43.2}.
**Fanuc-составы (v0.8, этап 10-подготовка — отчёт §2.4/§2.3):** семейство G40 += {G41.2–.6, G42.2–.6, G38, G39}; семейство G49 += {G43.4, G43.5, G44} (cancel также по H00); **новое семейство G11** (cancel G10-режима) = {G10, G10.6, G10.9, G10 L1/L10/L11/L20/L50-формы} — Raw-наследование по правилу семейств: если в файле есть RAW G10-формы (L50/L1…), `G11` → `RAW`; иначе `IGNORED_SAFE` (аналогия G80: запись уже завершена).

## 2. Матрица G-кодов

### 2.1 Движение

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | Fanuc (mill) | L1 | Примечание |
|---|---|---|---|---|---|---|---|
| G0 | NODE(rapid) | = | = | NODE(rapid) | = | rapid | Fanuc: та же диспозиция (группа 01) |
| G1 | NODE(linear) | = | = | NODE(linear) | = | linear | Fanuc: та же диспозиция; F модален, переживает G00 — как GRBL (§4 SPEC) |
| G2/G3 | NODE(arc) | = | = | NODE(arc) | = | arc | R и IJK → канон §6; helical — в обоих; G90.1/G91.1 → ABSORB. Fanuc: I/J/K — **инкрементальные от start всегда** (независимо от G90/G91); R-форма есть (знак R = ≤180°/>180°); R при полной окружности невалидна (alarm) — совпадает с SPEC §6(2); helical (perp-ось) и полное кольцо (end-слова опущены) легальны; эмиссия — in-plane IJK-пары (§5) |
| G5 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | N/A | — | кубич. B-spline (XY); **scope decision**: декомпозируем статически в linear, но меняет характер движения — canonical decomposition кандидат этапа 12 |
| G5.1 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | N/A* | — | квадр. B-spline; см. G5 — scope decision. *Лексема G05.1 в Fanuc занята (AI contour) → RAW(E_UNSUPPORTED_SYNTAX) — см. Fanuc-строку G05/G05.1/G08 ниже |
| G5.2/G5.3 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | N/A | — | NURBS (experimental); см. G5 — scope decision. Fanuc-NURBS = G06.2 — Fanuc-строка ниже |
| G33 / G33.1 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | RAW(E_UNSUPPORTED_SEMANTICS) (G33) / N/A (G33.1) | — | spindle-synced движение. Fanuc: G33 threading (+G34/G35/G36 variable/circular — тот же класс); G33.1 в словаре Fanuc нет (rigid tap = G84/G74 + M29) |
| G38.2/.3/.4/.5 | RAW(E_UNSUPPORTED_SEMANTICS) | = | = | RAW(E_UNSUPPORTED_SEMANTICS) | N/A (класс = G31/G37 — Fanuc-строка ниже) | — | пробинг — этап 12 |
| G80 | IGNORED_SAFE | = | = | IGNORED_SAFE | = | — | отмена cycle-моды; циклы в L1 немодальны. «Голая» координатная строка после G80 без motion-кода — `E_INVALID_INPUT` |
| G05/G05.1/G08 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SYNTAX) | — | **Fanuc**: AI contour / Nano smoothing — вендорские высокоточные режимы, вне статического контракта |
| G06.2 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** NURBS (опция); SPEC-план Фаза 1.2 уже включил NURBS в Raw-класс |
| G02.2/G03.2 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** эвольвентная интерполяция (опция); класс G5 (§2.1) — статически декомпозируемо в принципе, кандидат этапа 12 |
| G02.3/G03.3 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** экспоненциальная интерполяция (опция); см. G02.2 |
| G02.4/G03.4 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** 3D coordinate conversion (опция); 5-осевое |
| G07/G07.1 (G107) | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** гипотетическая ось / цилиндрическая интерполяция (опция); нелинейный маппинг осей |
| G12.1/G13.1 (G112/G113) | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** полярная интерполяция (опция); класс G07.1 |
| G31 / G31.x / G37 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) (G31.8+ → RAW(E_UNSUPPORTED_SYNTAX)) | — | **Fanuc** skip-функция (пробинг, опция) / EGB-skip / автоизмерение длины — сенсорные, класс G38.x; этап 12 |

### 2.2 Модальные режимы → ABSORB

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | Fanuc (mill) | L1 | Примечание |
|---|---|---|---|---|---|---|---|
| G17/G18/G19 | ABSORB (плоскость парсера) | = | = | ABSORB | = | — | валидация нормали дуги, §6. Fanuc: G18=ZX, G19=YZ — те же пары, что LinuxCNC |
| G20/G21 | ABSORB (units программы) | = | = | ABSORB | = | META(units) | переключение в середине → `E_UNSUPPORTED_SEMANTICS` (§3). Fanuc: одно переключение на программу (§2.5 SPEC); псевдонимы G70/G71 — Fanuc-строка ниже |
| G90/G91 | ABSORB (абсолютные координаты) | = | = | ABSORB | = | — | |
| G90.1/G91.1 | N/A (только G91.1) | = | = | ABSORB | ABSORB `?` (несверено) | — | центр дуги → канон §6. Fanuc: группа 04 — опция на 16i/30i; дефолт Fanuc = incremental (G91.1-эквивалент); наличие/группа на сериях — реестр §6 |
| G93/G94 | ABSORB (feed-разрешение §4) | = | = | ABSORB | G94 `=`; G93 ABSORB (опция) | state.feed | Fanuc G93 inverse-time (опция, 5-осевое) — `feed = len·F` §4 SPEC; G95 — Fanuc-строка ниже |
| G61 | META(exact_path) `?` | `?` | `?` | META(exact_path) | META(path_mode = exact_stop) | META | GRBL: код в словаре README — фактическое поведение сверить по source (§6.1). **Fanuc — сдвиг значения vs LinuxCNC:** Fanuc G61 = **exact stop** (останов в углу), LinuxCNC G61 = exact_path (G61.1 = exact_stop) — маппинг по значению, не по имени; §7 |
| G61.1 | N/A | `?` | `?` | META(exact_stop) | META(exact_stop) `?` (несверено) | META | Fanuc: точный стоп на части серий — синоним G61; групповая принадлежность (00 vs 13) на 0i-F — реестр §6 |
| G64 [P- Q-] | N/A | `?` | `?` | META(blend(t, q)) | META(blend, bare) | META | Q = Naive CAM Detector — в метаданные не входит, теряется с warning'ом. Fanuc: G64 **не имеет P-квантификатора** — только bare `blend` (легально по grammar v0.2+); t не существует в словаре |
| G95 | N/A | N/A | N/A | `?` (открытый вопрос) | RAW(E_UNSUPPORTED_SEMANTICS) | state.feed | **Мини-gap матрицы, закрыт v0.8 строкой (не решением):** LinuxCNC G95 (feed/rev) есть в словаре, строкой не покрывалась. Кандидат: ABSORB с разрешением `feed = F·S` при статическом S — ревизия по корпусу (реестр §6, §7). Fanuc: rpm фактический зависит от override → статическая эквивалентность не гарантируется |
| G70/G71 | N/A | N/A | N/A | N/A | META(units) | units | **Fanuc mill-псевдонимы G20/G21** — распознавать. Коллизия с токарными циклами G70/G71 (finishing/roughing на T-машине) — профиль различает M/T-контекст |
| G60 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** одномерное позиционирование (overtravel-подход, one-shot); меняет геометрию подхода смещением по одной оси — не выражимо |
| G09 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** exact stop one-shot на следующий блок; path_mode в L1 — программно-глобальный (§3 SPEC), one-shot останов не представим (соседний G61 → META exact_stop) |
| G62 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** automatic corner override — подача режется по углу из параметров станка; process intent, не представимо |
| G63 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** tapping mode; paired с G84 |
| G15/G16 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** полярные координаты (опция); трансформация СК — класс G68 (§2.6) |
| G22/G23 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** stored stroke check on/off — софт-лимиты из параметров станка; машина, не программа |
| G25/G26 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SYNTAX) | — | **Fanuc** spindle fluctuation detect — мониторинг, вне движения |

### 2.3 Системы координат и фреймы

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | Fanuc (mill) | L1 | Примечание |
|---|---|---|---|---|---|---|---|
| G54–G59 | ABSORB (wcs) | = | = | ABSORB | = | state.wcs | Fanuc: та же диспозиция; 6 WCS, p=1..6 ⊂ EID p=1..9 |
| G59.1/.2/.3 | N/A (нет в словаре) | `?` | `?` | ABSORB (wcs) | N/A | state.wcs | эмиссия в GRBL невозможна — §5. Fanuc: кодов нет (G54.1 P1..P3 — другая таблица с чужим происхождением origin); эмиссионная сторона — §5.1 |
| G54.1 Pn | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **отсутствует в словаре обоих MVP-диалектов** (официальная документация LinuxCNC: только G54–G59.3). **Fanuc (v0.8): присутствует, диспозиция RAW** — доп. WCS P1–P48 (опции до P300), группа 12; выбор СК вне wcs-enum L1; кандидат ревизии этапа 12 (обобщение wcs-enum / p-домена, обоих направлений); обратная эмиссия — §5.1 |
| G10 L2 | NODE(wcs_define) | = | = | NODE(wcs_define) | = (L2 P1–P6) | wcs_define | разрешён в любой позиции (§5.3). Fanuc: 1:1, P1..P6 ↔ p=1..6; G90/G91 — абсолют/инкремент записи (§5.3 SPEC); P0 — Fanuc-строка ниже |
| G10 L20 | NODE(wcs_set_here) | = | = | NODE(wcs_set_here) | **RAW(E_REQUIRES_MACHINE_TABLE)** — Fanuc-семантика, НЕ wcs_set_here | — | только пресетка до первого motion-опа (GRBL/LinuxCNC). **Fanuc — ловушка №3:** тот же код L20, семантика другая — запись origin доп. СК **G54.1 Pn** (расширение L2: absolute/incremental), а НЕ «set current position as origin». **Fanuc-парсеру запрещено маппить L20 → wcs_set_here**; реальное «set here» в Fanuc-идеоматике — G92/G10-комбинации вне текста; эмиссия wcs_set_here → Fanuc запрещена (§5.1) |
| G10 L1/L10/L11/L28/… | N/A | `?` | `?` | RAW(E_REQUIRES_MACHINE_TABLE) | RAW(E_REQUIRES_MACHINE_TABLE) (L1/L10/L11/L50+) | — | записи таблиц инструментов / прочие. Fanuc: + параметры NC (L50+) |
| G10 L2 P0 | N/A | N/A | N/A | N/A | RAW(E_REQUIRES_MACHINE_TABLE) | — | **Fanuc** external/common offset — общий сдвиг поверх всех WCS; смена эффективного фрейма всех СК; понятие отсутствует в L1 |
| G92 | META(frame_preset) | = | = | META(frame_preset) | = (X_ Y_ Z_) | frame_preset | только пресетка; координаты, присваиваемые текущей позиции (§3). Fanuc: 1:1; сдвиг **переживает конец программы** (сброс — reset/power off/G92.1) — на эквивалентность не влияет (EQUIVALENCE §1.2), эмиссии hygiene — §5.1; S-слово при G92 — Fanuc-строка §2.6 |
| G92.1 | RAW(E_UNSUPPORTED_SEMANTICS) | = | = | RAW(E_UNSUPPORTED_SEMANTICS) | = | — | Q4 |
| G92.2/G92.3 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | RAW(E_UNSUPPORTED_SEMANTICS) `?` (несверено) | — | Q4. Fanuc: наличие на mill-сериях — реестр §6 |
| G53 | NODE-поле `frame=machine` | = | = | NODE-поле | = | state.frame | правило machine-frame §5.3 (origin обязан быть in-text) |
| G52 | N/A | N/A | N/A | N/A | **ERR(E_UNSUPPORTED_SEMANTICS)** | — | **Fanuc** локальная СК (сдвиг поверх активной WCS) — **единственный жёсткий отказ Fanuc-матрицы** (класс SPEC §8 правило 5): позиционно-зависимая смена фрейма mid-program — raw-сохранение исказило бы последующие ходы. Отмена `G52 X0 Y0 Z0` неявная (невыписанные оси сохраняют сдвиг). Кандидат этапа 12: декомпозиция СДВИГА статически возможна, но семантика «локальной СК» с WCS-переключениями внутри G52-зоны — нет |
| G28 / G30 | RAW(E_REQUIRES_MACHINE_TABLE) | = | = | RAW(E_REQUIRES_MACHINE_TABLE) | = (G28 через промежуточную точку) | — | Q1; узел — этап 8. Fanuc: синтаксис `G28 X_ Y_ Z_` — промежуточная точка в словах осей (упрощение: «до-этого»-путь можно опустить); позиция reference из параметров → вне текста |
| G28.1 / G30.1 | RAW(E_REQUIRES_MACHINE_TABLE) | = | = | RAW(E_REQUIRES_MACHINE_TABLE) | G28.1 N/A; G30.1 RAW(E_REQUIRES_MACHINE_TABLE) `?` (несверено) | — | запись home из текущей позиции — тот же класс, что Q1. Fanuc: G30.1 floating reference `?` |
| G27 | N/A | N/A | N/A | N/A | RAW(E_REQUIRES_MACHINE_TABLE) | — | **Fanuc** reference return check (one-shot); home-класс Q1 |
| G29 | N/A | N/A | N/A | N/A | RAW(E_REQUIRES_MACHINE_TABLE) | — | **Fanuc** от reference в точку; позиция reference вне текста |
| G30 P2/P3/P4 | N/A | N/A | N/A | N/A | RAW(E_REQUIRES_MACHINE_TABLE) | — | **Fanuc** 2nd–4th reference return; Q1-класс |

### 2.4 Компенсации

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | Fanuc (mill) | L1 | Примечание |
|---|---|---|---|---|---|---|---|
| G40 | IGNORED_SAFE* | = | = | IGNORED_SAFE* | = | — | *правило семейств §1.3. Fanuc: состав семейства шире (§1.3 Fanuc-составы) |
| G41/G42/G41.1/G42.1 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | RAW(E_UNSUPPORTED_SEMANTICS) (G41/G42 +D) | — | применение компенсации геометрией — отдельная фича (не-цель §1.2). Fanuc: D = номер радиуса в таблице (значение из таблицы); +G41.2–.6/G42.2–.6 и G38/G39 — тот же класс, входят в семейство G40 (§1.3) |
| G43 H | N/A | RAW(E_REQUIRES_MACHINE_TABLE) `?` | `?` | RAW(E_REQUIRES_MACHINE_TABLE) | = | — | TLO из таблицы. Fanuc: H модален; G43 может появляться без H (переактивация старого H); geometry+wear в таблице |
| G43.1 | RAW(E_UNSUPPORTED_SEMANTICS) | RAW(E_UNSUPPORTED_SEMANTICS) | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | RAW(E_REQUIRES_MACHINE_TABLE) `?` (несверено) | — | динамический TLO; **кандидат ревизии**: значение в тексте → разрешаемое поле `tlo` (Z-сдвиг). Триггер — **первое появление в Tier B**, не частотность (ревью §7.6). Fanuc: dynamic TLO (опция) — тот же кандидат `tlo`; TCP G43.4/.5 — Fanuc-строка ниже |
| G43.2 | N/A | RAW(E_REQUIRES_MACHINE_TABLE) `?` | `?` | RAW(E_REQUIRES_MACHINE_TABLE) `?` | N/A (нет в словаре Fanuc) | — | дополнительный TLO; семантика (табличная vs динамическая) — сверить, §6; входит в семейство G49 (B3) |
| G44 | N/A | N/A | N/A | N/A | RAW(E_REQUIRES_MACHINE_TABLE) | — | **Fanuc** TLO − (H); редкая: обычно компенсируют отрицательным значением в H; семейство G49 |
| G43.4/G43.5 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** TCP / tool axis direction control (опции); 5-осевое — этап 12; семейство G49 |
| G49 | IGNORED_SAFE* | = | = | IGNORED_SAFE* | = (или H00) | — | *правило семейств §1.3. Fanuc: cancel также по H00; Fanuc-состав семейства — §1.3 |

### 2.5 Циклы (LinuxCNC, Fanuc; GRBL-семейство — N/A)

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | Fanuc (mill) | L1 | Примечание |
|---|---|---|---|---|---|---|---|
| G81/G82/G83/G85/G86/G89 | N/A | N/A | N/A | разворачиваются (§7 SPEC) | разворачиваются (§7 SPEC, Fanuc-семантика отчёта §5.4) | rapid/linear/dwell | G86 — рестарт шпинделя обязателен (Q9 закрыт). **Fanuc: P dwell — целые миллисекунды, decimal point запрещён** (`G04`/цикл; конверсия ms→s при канонизации); G83 — полный отвод к R, возврат до `depth − δ`; K/L-повторы (0–9999) разворачиваются; **K0 в G91 — Fanuc-строка ниже** |
| G73 | N/A | N/A | N/A | разворачивается | разворачивается — **Fanuc-семантика отличается** | rapid/linear | chip-break: отвод над последней глубиной. Fanuc: отвод между пеками = **Q** (быстрый), не фикс. 0.5 мм как LinuxCNC; rapid вниз до `последняя_глубина − δ`, δ = параметр станка (5100-серия `?`) — разворачивать **по семантике источника**; δ → канон-константа 0.5 мм + notice `W_CYCLE_PARAM_ASSUMED` (влияет только на traverse, не на cut-геометрию; AST GRBL↔Fanuc-развёрток расходятся в «подход»-ходах по построению — R3 Tier A не затронут) |
| G84/G74 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | = | — | нарезание резьбы, синхронизация. Fanuc: rigid-варианты G84.2/G84.3 (и G76.1 `?`) — тот же класс; M29 paired — Fanuc-строка §3 |
| G76 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | RAW(E_UNSUPPORTED_SEMANTICS) | — | LinuxCNC G76 = **токарный цикл резьбы** (ZX, синхронизация) → вне скоупа §1.2. **Fanuc G76 (v0.8) = fine boring с Q-shift → RAW: двойной отказ** — (а) ориентация шпинделя на дне (внутр. M19-эквивалент, M19-класс), (б) направление сдвига = **параметр станка 5101 bits 4/5, не программа** (значение вне текста). **Q6 закрыт research'ом (v0.8)**; source-чек bits 5101 — остаток, реестр §6 (несверено); P/P2-дополнения на 30i `?` — там же |
| G87/G88 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | = | — | LinuxCNC: «accepted, behavior undefined». Fanuc: G87 — семантика полноценная (ориентация шпинделя + сдвиг I/J + ручное вмешательство) но не представима ходами; G88 — manual stop (feed hold + джог), не статично |
| G98/G99 | N/A | N/A | N/A | ABSORB (правило §7.6) | = (группа 10 mill) | — | initial Z требует определённой позиции, иначе `E_RUNTIME_STATE`. Fanuc: дефолт без G98/G99 — `?` (исторически G99 на 0i; реестр §6; LinuxCNC-сторона закрыта source'ом). **Латhe-коллизия:** на Fanuc-lathe G98/G99 = группа 05 (feed-режимы) — смешение lathe/mill-файлов → `E_INVALID_INPUT` на противоречивой интерпретации; профиль различает M/T-контекст |
| K0/L0 в G91 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** спец-семантика «движение по одной оси» (цикл сохранён, одномерный ход) `?` (несверено) — редкая идиома; v0.1: встречено → raw-диагностика, разворачивание не делается; точные условия (manual B-61863) — реестр §6 |
| G80.5/G80.8, G81.1, G81.5/.8 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SYNTAX) | — | **Fanuc** вендорские опции 30i (high-speed retract, chopping, EGB) |

### 2.6 Прочее

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | Fanuc (mill) | L1 | Примечание |
|---|---|---|---|---|---|---|---|
| G4 | NODE(dwell) | = | = | NODE(dwell) | NODE(dwell) (P = целые мс!) | dwell | P секунды в GRBL и LinuxCNC — подтверждено (grbl interface.md; LinuxCNC gcode.html). **Fanuc:** P — **целые миллисекунды, decimal point запрещён** (`G04 P500` = 0.5 s; конверсия ms→s при канонизации); X/U = секунды float (X/U-форма 1:1); обратная эмиссия — §5.1 (ms-целое → P, sub-ms → X); `G04 P0` — drop+notice (SPEC §3/§5.2) |
| G96/G97 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | = | — | CSS — токарное, вне скоупа §1.2 (Fanuc группа 15) |
| G7/G8 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | N/A (G08 в Fanuc занят — Nano smoothing → RAW(E_UNSUPPORTED_SYNTAX), §2.1) | — | диаметральный режим — токарное |
| G50/G51 | N/A | N/A | N/A | N/A `?` | RAW(E_UNSUPPORTED_SEMANTICS) | — | scaling: в стабильной документации LinuxCNC отсутствует, legacy 2.4–2.5 содержал — сверить (§6). **Fanuc:** `G51 X_ Y_ P_` (центр, фактор; отрицат. P = зеркало) / `G50` cancel; группа `?` (00 vs 18 по источникам) — реестр §6; параметры в тексте → декомпозиция в принципе возможна, кандидат этапа 12 (см. G68) |
| G92 S / G50 S | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** кламп max оборотов шпинделя — **тот же адрес G92/G50, другой word**: парсер обязан различать (S-слово при G92 = НЕ frame_preset) |
| G10.6 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** retract & recover — выгрузка/возврат инструмента |
| G10.9 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** diameter/radius switch (токарное-родственное) |
| G45–G48 | N/A | N/A | N/A | N/A | RAW(E_REQUIRES_MACHINE_TABLE) | — | **Fanuc** позиционирование ±H-offset (1×/2×) — смещение хода на табличное значение H; класс G43 |
| G51.2/G50.2 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** polygon turning (токарное) |
| G68/G69 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** поворот СК `G68 X_ Y_ R_` (центр, угол°) / `G69` cancel (группа 16); параметры в тексте → статически декомпозируем в принципе, кандидат этапа 12; осложнение: G68-зона может содержать WCS-переключения и G52 → интерференция трансформаций; «запекание» делает канонизацию зависимой от трансформации (аналогия G5 scope decision) |
| G50.1/G51.1 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** программируемое зеркало по оси `G51.1 X0` (группа 17); см. G68. Зеркало **меняет сторону фрезерования** (climb↔conventional) — «free»-декомпозиция искажает process intent — дополнительный аргумент против ранней декомпозиции |
| G72.1/G72.2 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** figure copy rotation/linear (опции) — копирование фигуры = авторазвёртка подпрограммы, вне L1 |

## 3. Матрица M-кодов

Fanuc (v0.8): **фиксированную** семантику по всем Fanuc имеют только M00/M01/M02/M30/M98/M99 (+стандартные M03–M09/M06); остальные — machine-builder specific. Неизвестный M-код (вне словаря профиля) → `RAW(E_UNSUPPORTED_SYNTAX)` с диагностикой — не молча (generic-ведро, строка ниже).

| Код | GRBL 1.1 | grblHAL | FluidNC | LinuxCNC | Fanuc (mill) | L1 | Примечание |
|---|---|---|---|---|---|---|---|
| M0 | NODE(program_stop) | = | = | NODE(program_stop) | = | program_stop | optional=false |
| M1 | NODE(program_stop) | = | = | NODE(program_stop) | = | program_stop | optional=true; Q8 (настройка станка вне L1) |
| M2 / M30 | NODE(program_end) | = | = | NODE(program_end) | = | program_end | различие M2/M30 — поведение контроллера после конца, вне L1. Fanuc: дефолт эмиссии — M30 (§5.1) |
| M3/M4/M5 | ABSORB (spindle) | = | = | ABSORB | = | state.spindle | синтаксис `$n` (LinuxCNC multi-spindle): `$n` ≠ default → `E_UNSUPPORTED_SEMANTICS` (multi-spindle вне скоупа v0.0). Fanuc: S-слово без M03 — только сохранение оборотов |
| M6 | N/A → RAW(E_UNSUPPORTED_SYNTAX) | NODE(tool_change) | NODE(tool_change) `?` | NODE(tool_change) | NODE(tool_change) (идиома `T05 M06` в одном блоке) | tool_change | LinuxCNC M6 не меняет TLO (G43 отдельно) — совместимо с узлом; Fanuc M6 тоже не применяет TLO (применяет G43) — restart при T=M-номеру `?` (реестр §6). Fanuc: T-слово отдельно = **предвыбор** → RAW(E_UNSUPPORTED_SEMANTICS), см. T-слово |
| M7/M8/M9 | ABSORB (coolant) | = | = | ABSORB | = (M07 mist — опция) | state.coolant | |
| M19 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | = (R/Q/P углы) | — | ориентация (R/Q/P), нужен энкодер. Fanuc G76-зависимость закрыта v0.8 (G76 = RAW, параметр 5101 — §2.5) |
| M48/M49 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | N/A | — | override — process intent (условия исполнения, напр. disable перед растачиванием); same-dialect fidelity требует сохранения (ревью §7.4) |
| M50/M51 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | N/A | — | тот же класс, что M48/M49 |
| M52/M53 | N/A | `?` | `?` | RAW(E_RUNTIME_STATE) | N/A | — | adaptive feed = вход HAL в рантайме; feed-hold switch |
| M60 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | N/A | — | pallet change + pause |
| M61 | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | RAW(E_UNSUPPORTED_SEMANTICS) `?` | RAW(E_UNSUPPORTED_SEMANTICS) | N/A | — | номер без физической смены не соответствует семантике `state.tool` (физический, SPEC §4); ABSORB «врал» бы о физике. Кандидат ревизии — поле `selected_tool` (ревью §7.5) |
| M62–M65 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | N/A (builder-specific — generic-ведро ниже) | — | цифровые выходы |
| M66 | N/A | `?` | `?` | RAW(E_RUNTIME_STATE) | N/A | — | ожидание входа |
| M67/M68 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | N/A | — | аналоговые выходы |
| M70–M73 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | N/A | — | стек модального состояния; в L1 нет состояния-потока; кандидата до L2 нет |
| M98/M99 | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SYNTAX) | = (M98 P_ [L_/K_]; M99 [P_]) | — | Fanuc-style subs; **Q10**: статический инлайнинг без параметров — кандидат ревизии по Tier B. Fanuc: вложенность 4 уровня (subs+macros суммарно); `M98 P<L><####>` слип `?` (реестр §6); M99 в main = бесконечный цикл — структурно не статичен; часть Macro B-механики (§4) |
| M198 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SYNTAX) | — | **Fanuc** вызов внешней подпрограммы (карта/DNC) — вне файла → не статично вдвойне |
| M29 | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Fanuc** rigid tapping mode (builder-распространённый) — paired с G84/G74 |
| M100–M199 | N/A | `?` | `?` | RAW(E_UNSUPPORTED_SEMANTICS) | RAW(E_UNSUPPORTED_SYNTAX) (builder-specific) | — | внешние executables — вне вопроса |
| M вне словаря профиля (M64–M69, M90–M99+ и пр.) | N/A | N/A | N/A | N/A | RAW(E_UNSUPPORTED_SYNTAX) | — | **Fanuc generic-ведро**: любой builder-specific M (I/O, pallets, станочные макро-вызовы) + диагностика текста — не молча |
| T-слово | ABSORB (state.tool, только в паре с M6; одиночное T — `E_INVALID_INPUT`) | `?` — семантика T-без-M6 при наличии M6-семейства, §6.8 | `?` — §6.8 | ABSORB (T без M6 — **предвыбор**: RAW(E_UNSUPPORTED_SEMANTICS)) | ABSORB (только в паре T..M6); одиночное T — RAW(E_UNSUPPORTED_SEMANTICS) (предвыбор) | state.tool | одиночное T в LinuxCNC легально (предвыбор) — не E_INVALID_INPUT (исправлено ревью); кандидат ревизии `selected_tool` (ревью §7.5). Fanuc: T отдельно = modal next-tool (предвыбор) — тот же класс |

## 4. Прочий словарь

| Элемент | Диспозиция | Примечание |
|---|---|---|
| N-слова | IGNORED_SAFE ×все | номера кадров. Fanuc: в Macro B-файлах N = цель GOTO — но там строка уже RAW (§4, Macro B) |
| `%` маркеры программы | IGNORED_SAFE ×все | Fanuc: DNC-маркер начала/конца файла; файл без `%` и без M30 (DNC-выгрузка) — обычный случай, auto-PEND (v0.16) применим |
| Комментарии `()` и `;` | аннотации (SPEC §11) | не в матрице отказов. Fanuc: **только `( )`** (вложенность запрещена); `;` — НЕ комментарий, а EOB — Fanuc-строка ниже |
| `/` block delete | LinuxCNC/Fanuc: RAW(E_RUNTIME_STATE); GRBL: N/A `?` (§6) | исполнение зависит от переключателя станка — не статично. Fanuc: поддерживается, тот же класс |
| `$`-строки (GRBL config) | RAW(E_UNSUPPORTED_SYNTAX) | конфигурация станка, не программа |
| Слова A/B/C/U/V/W | RAW(E_UNSUPPORTED_SEMANTICS) ×все | ротационные и линейные 4-е оси — этап 12 (UVW добавлены ревью). Fanuc: **исключение — U в G04 = dwell-секунды** (разобрать контекстно до общего U-отказа) |
| **EOB `;` (Fanuc)** | **разделитель блоков** (эквивалент `\n`) или `E_INVALID_INPUT` — политика адаптера | **Ловушка №2:** лексеры GRBL/LinuxCNC трактуют `;` как начало комментария — молча выбросить хвост строки как «комментарий» = **потеря кодов**. Fanuc-адаптер обязан считать `;` разделителем строк; трактовка «комментарий» для Fanuc-входа запрещена |
| **Bare-integer без decimal point (DPI, Fanuc)** | DPI=0-чтение (least increment) + notice, ИЛИ `E_INVALID_INPUT` — политика v0.1 (рекомендация отчёта §9.1), ревью | **Ловушка №1:** DPI=0 — документированный дефолт Fanuc: `X25` = 25 × least increment = **0.025 мм на IS-B**; молча читать как целое **запрещено** (тихая ошибка ×1000). Слова с дробным доменом: X/Y/Z/R/Q/F/I/J/K. Трейлинг-точка `X50.` легальна — лексер обязан принимать. DPI-дефолт на 0i-F — реестр §6 |
| **O-слова `O####` (Fanuc)** | RAW(E_UNSUPPORTED_SYNTAX) | номер программы / подпрограммы (main + subs в одном файле). Хвост-subs после program_end → **raw-хвост целиком** (E_UNSUPPORTED_SYNTAX; не Err — продолжение текста, а не искажение интерпретации предшествующих опов; диагностика фиксируется). Детект «последний O-код = subs» для auto-PEND `?` — реестр §6 |
| **Macro B-семейство (Fanuc)** | line-level RAW(E_UNSUPPORTED_SYNTAX) ×все: `#`-переменные (все диапазоны #0–#13400), `[ ]`-выражения, IF/GOTO/THEN, WHILE/DO/END, SETVN, G65/G66/G66.1/G67 | Fanuc параметрика — вне грамматики L1 (v0.16); **декомпозиция запрещена** — L2, этап 12. Чтение system vars (#1000+, #4xxx-модальные, #5xxx-позиции) по смыслу E_RUNTIME_STATE, но в матрице v0.1 не различается от SYNTAX-строк (принцип: не декомпозиция, Raw). Соседние статические ходы парсятся как обычно (точки разрыва → unknown); кросс-диалект при любом Raw → `E_RAW_UNTRANSLATABLE`; same-dialect — R1-text байт-в-байт. Известное ограничение: условная смена фрейма/единиц в Macro B не детектируется (R1-text честен текстом; эквивалентность движения не заявляется) |
| **H/D-слова (Fanuc)** | RAW(E_REQUIRES_MACHINE_TABLE) — в составе G43/G41-строк | номера смещений длины/радиуса из таблиц станка |
| **E-слово (Fanuc)** | RAW(E_UNSUPPORTED_SYNTAX) | точность G05.1 (вендорский режим) |
| **Блок координат без motion-кода (Fanuc)** | валидно — ABSORB модальности (продолжение группы 01) | адресный язык Fanuc: `X100.` без G-кода легален — как GRBL/LinuxCNC; порядок слов в блоке произвольный (парсер адресных слов, не позиционный); пустые блоки легальны — пропуск без диагностики |

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
| **хвост M5/M9 перед program_end** | **не эмитится** (ruling, итерация 4): AST не хранит пост-конечное состояние (SPEC §5.2), M2/M30 обоих контроллеров сами глушат шпиндель/СОЖ (safety reset по документации) — избыточные M5/M9 = эстетикаlegacy-постов, не семантика. Симуляция подтверждает эквивалентность (E, e2e) | = | = | = |
| циклы (развёрнутые ходы) | само собой | = | = | развёрнутый вывод — документированный verbose (SPEC §1.2) |

### 5.1 Эмиссия → Fanuc (reverse-gap; v0.8, из отчёта §8 — ревью)

Правило класса отказа — §5 (вне словаря цели → SYNTAX; не представимо → SEMANTICS).

| Узел/мета | → Fanuc (mill) | Примечание |
|---|---|---|
| `state.wcs` = G59.1/.2/.3 | отказ `E_UNSUPPORTED_SYNTAX` (кодов нет) | Fanuc-эквивалент G54.1 P1..P3 — **другая таблица** с чужим происхождением origin. Профиль-опция «map G59.x → G54.1 Pn» с notice — этап 10, НЕ дефолт (подмена таблицы без предупреждения запрещена — аналогия `golden_preset_no_bake`) |
| wcs_set_here (G10 L20 LinuxCNC-семантика) | **эмиссия запрещена как L20** → отказ `E_UNSUPPORTED_SEMANTICS` | Fanuc L20 = запись origin G54.1 Pn — другое (§2.3); «set here» через G92-комбинации требует машинной позиции M — не статично → честный отказ |
| path_mode exact_path | warning + ближайший `G61` (консервативно) ИЛИ bare `G64` — выбор профиля | Fanuc G61 = exact stop, не exact_path; G64 = blend; потери в source map |
| path_mode exact_stop | `G61` | сдвиг значения vs LinuxCNC (§2.2) |
| path_mode blend(t) | bare `G64` + warning (t теряется) | P-квантификатора в словаре Fanuc нет |
| dwell | `P = round(seconds·1000)` — если остаток в допуске dwell (EQUIVALENCE §4: floor 0.0001 s) → **X-форма** (`G04 X0.0005`); правило: ms-целое → P, иначе → X | P — целые мс, decimal point запрещён; sub-ms P-формой не представим; Fanuc X принимает float-секунды |
| `state.feed` (включая G93/G95-разрешения) | эмиссия всегда `G94` (EQUIVALENCE §2) | Fanuc G94 есть |
| arc — полное кольцо (sweep=360°, pitch=0) | `G02/G03 I_ J_` **без end-слов** | Fanuc легален тот же формат, что GRBL (§5); слова end==start при перепарсе квантуются в zero-length drop; винтовой полный оборот (pitch≠0) — с end-словом оси |
| arc R-форма | всегда IJK; R — только флаг профиля + notice (EQUIVALENCE §4) | Fanuc R поддерживает, но канонический контракта — IJK |
| числа | decimal point **ВСЕГДА** (DPI-ловушка §4); округление до least increment цели с notice («задокументированное округление» SPEC §10.1) | IS-B mm = 3dp значимы; трейлинг-нули Fanuc терпит |
| program_end | M30 (дефолт, Fanuc-конвенция) vs M02 — выбор профиля | auto-PEND (v0.16) → авто-вставка M30 применим к DNC-файлам без M30 (хвост-subs-случай — §4) |
| frame_preset G92 | `G92 X_ Y_ Z_` первым блоком; порядок §1.4 EQUIVALENCE; G92 S-конфликта нет (S-слова в пресетке не эмитятся) | hygiene-опция профиля: `G92.1` первым блоком (notice) — НЕ дефолт (меняет глобальное состояние станка — операторский контракт EQUIVALENCE §1.2) |
| `state.tool` | `T## M06` | 1:1 |
| хвост M5/M9 перед program_end | не эмитится (ruling §5 — применим) | Fanuc M30 сам глушит шпиндель/СОЖ |
| циклы (развёрнутые ходы) | Fanuc принимает развёрнутые ходы | GRBL→Fanuc даст verbose — документировано (SPEC §1.2) |

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
13. **LinuxCNC: документированный дефолт retract-режима (G98 vs G99) — чек выполнен (Agent D, итерация 3-4): дефолт в стабильной документации НЕ задокументирован** (g-code.html §61 + §43.4, цитаты в отчёте D) — противоречий §7.5a (provisional G99) нет. Остаток закрыт ✔ **(source-чек rs274ngc master, 2026-11-21, Agent D)**: fallback-инициализация retract mode = **R_PLANE/G99** (`interp_setup.cc:152`; enum RETRACT_MODE {R_PLANE, OLD_Z} interp_internal.hh) — документацией дефолт не заявлен, зафиксирован source'ом. **SPEC §7.5a подтверждена** (G99-provisional → подтверждено).
14. **Адекватность константы подхода 0.5mm (0.05in)** для реальной практики G73 — верифицировать по LinuxCNC-doc/практике. **Профиль константу НЕ переопределяет** (SPEC §7.4a v0.14.1: детерминизм канонической формы); несоответствие практики = ревизия константы через CHANGELOG, не параметр.
15. **Fanuc G90.1/G91.1 (несверено)** — наличие и группа (04) на 0i-D/F vs 16i/18i/21i/30i; поведение IJK по умолчанию (инкремент подтверждён практикой, source-check нет).
16. **Fanuc K vs L в canned cycles** — какие серии принимают L как повтор цикла (0–9999) наряду с K.
17. **Fanuc K0/L0 в G91 (несверено)** — спец-семантика «движение по одной оси»: точные условия (manual B-61863/…).
18. **Fanuc G76 P/P2-слова (несверено)** — на 30i G76 имеет P/P2-дополнения (dwells) `?`; Q-shift bits параметра 5101 — подтверждены практикой (Practical Machinist/eMastercam), source-check нет. Остаток закрытия **Q6** (строка §2.5).
19. **Fanuc G73/G83 clearance δ (несверено)** — номер параметра (5100-серия) и дефолт на 0i-D/F; до сверки — константа 0.5 мм + notice `W_CYCLE_PARAM_ASSUMED`.
20. **Fanuc G86 без P (несверено)** — останавливает ли Fanuc G86 шпиндель без dwell (LinuxCNC: «dwell P (если задан)»); рестарт-семантика на конкретной серии.
21. **Fanuc дефолт G98/G99 на 0i mill (несверено)** — практика: G99; source недоступен, сверка только по manual (LinuxCNC-сторона закрыта — §6.13 ✔).
22. **Fanuc M98 P\<repeat\>\<program\> (несверено)** — слип-формат (P51000 = 5× O1000): на каких сериях; повтор L vs K.
23. **Fanuc G61.1/G09 (несверено)** — точная групповая принадлежность на 0i-F (G61.1 — группа 00 или 13).
24. **Fanuc G50/G51 (несверено)** — группа 00 vs 18 по сериям.
25. **Fanuc G92.2/G92.3 (несверено)** — наличие на mill-сериях.
26. **Fanuc IS-C inch 5dp вход** — политика notice-vs-reject (решение ревью): EID-квантование 4dp теряет 0.00001"; невозможна на 0i — notice-класс при обнаружении 5dp в inch-файле.
27. **Fanuc DPI-дефолт на современных 0i-F (несверено)** — практика: DPI=0 legacy; calculator-режим популярен — политика строки DPI (§4) после сверки.
28. **Fanuc «последний O-код = subs» (несверено)** — надёжность детекта хвоста для auto-PEND (§4, строка O-слов).
29. **Fanuc M06 и TLO (несверено)** — M6 не применяет TLO (применяет G43) — совместимо с узлом tool_change; сверить поведение restart при T=M-номеру.
30. **LinuxCNC G95 — мини-gap матрицы (закрыт v0.8 строкой с `?`)** — кандидат ABSORB с разрешением `feed = F·S` при статическом S; ревизия по корпусу (открытый вопрос, НЕ принятое решение); Fanuc-сторона = RAW (§2.2).
31. **Fanuc F-адрес в DPI-домене — РЕШЕНО ревью этапа-1 eid-fanuc (2026-11-21)**: F имеет собственный инкремент **1 mm/min** (параметр 1422): bare `F100` = 100 mm/min, НЕ 0.1. F исключён из ×0.001-DPI-домена (остаются X/Y/Z/R/Q/I/J/K); notice W_DPI_ASSUMED сохраняется. G95 (mm/rev) → Raw, его инкремент не нужен. Поправка к `fanuc_dialect_analysis.md` §1.2 внесена.
32. **Fanuc хвост после program_end (решено этапом-1 eid-fanuc)**: блоки после M2/M30 → **Raw-хвост {E_UNSUPPORTED_SYNTAX}** (subs-тела после M30 — норма Fanuc-файлов, анализ §4.4/§7.3); осознанное диалект-расхождение с GRBL n12 (там Err). Downstream: Q11 даёт start=unknown на точке разрыва; `G43 H2 Z25.` → whole-line Raw (потеря хода — известное ограничение, split-parse запрещён как скрытая декомпозиция).

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
10. **G54.1 Pn — отсутствует в словаре обоих MVP-диалектов** (уточнение к твоему чек-листу: это не LinuxCNC-код; официальная документация LinuxCNC знает только G54–G59.3). Fanuc/HAAS — этап 10; **Fanuc-диспозиция внесена v0.8**: RAW(E_UNSUPPORTED_SEMANTICS) (§2.3; выбор) + G10 L20 Fanuc-семантика = RAW(E_REQUIRES_MACHINE_TABLE) (запись, НЕ wcs_set_here).
11. **Fanuc G61 = exact_stop — сдвиг значения vs LinuxCNC G61 = exact_path (v0.8, не конфликт строк):** существующая LinuxCNC-строка §2.2 не переписывалась — различие в разных колонках; маппинг по значению, не по имени. Эмиссионное следствие: exact_path → Fanuc эквивалента нет (warning + G61-консервативно или bare G64 — профиль, §5.1).
12. **G95-мини-gap (v0.8):** строка добавлена с `?` для LinuxCNC — **открытый вопрос, НЕ принятое решение** (кандидат ABSORB `feed = F·S` при статическом S, ревизия по корпусу); Fanuc-сторона = RAW (§2.2).
13. **Q6 закрыт research'ом (v0.8, G76):** Fanuc G76 = RAW(E_UNSUPPORTED_SEMANTICS) — двойной отказ: (а) ориентация шпинделя на дне (внутр. M19-эквивалент), (б) Q-shift = параметр станка 5101 bits 4/5 (значение вне текста). Source-чек bits — остаток в реестре §6.18, строка считается несверенной до приёмки.
14. **Fanuc-семейства (v0.8, §1.3):** G40/G49 составы расширены, введено семейство G11 = {G10-формы} с IgnoredSafe→Raw-наследованием — ревью составов при приёмке Fanuc-колонки.

## 8. Источники

- [Grbl 1.1 README (словарь кодов)](https://github.com/gnea/grbl/blob/master/README.md)
- [Grbl interface.md (G4 P секунды, слова)](https://github.com/gnea/grbl/blob/master/doc/markdown/interface.md)
- [Grbl v1.1 Commands wiki](https://github.com/gnea/grbl/wiki/Grbl-v1.1-Commands)
- [grblHAL core wiki (G43*/G43.1/G43.2/G49, M6, M61)](https://github.com/grblHAL/core)
- [LinuxCNC G-code overview](https://linuxcnc.org/docs/stable/html/gcode/g-code.html)
- [LinuxCNC M-code overview](https://linuxcnc.org/docs/stable/html/gcode/m-code.html)
- [DryRun CNC — GRBL 1.1 / grblHAL supported G-codes](https://dryruncnc.com/docs/grbl)
- [FluidNC wiki — supported G-codes](https://wiki.fluidnc.com/en/stable/features/paths) (дельты, `?`)
- Fanuc-колонка (v0.8): `fanuc_dialect_analysis.md` v0.0 §11 — [Helman CNC — Fanuc 30i/31i/32i G-codes](https://www.helmancnc.com/fanuc-31i-g-codes-machining-center-fanuc-30i-31i-32i/); [Helman CNC — decimal point (DPI)](https://www.helmancnc.com/fanuc-decimal-point-programming-introduction-parameters); [CNCCookbook — Fanuc Macro B system variables](https://www.cnccookbook.com/fanuc-macro-system-variables-cnc-g-code-parameterized-programming); [CNCCookbook — G98/G99 mill vs lathe](https://www.cnccookbook.com/g98-g-code-g99-g-code-canned-cycle-return-feedrate-modes); [GCodeTutor — M98/M99 subprograms](https://gcodetutor.com/cnc-macro-programming/subprograms-m98-m99.html); [GCodeTutor — G73/G83 peck cycles](https://gcodetutor.com/fanuc-training-course/g73-g83-drilling-cycle.html); [cncmacros.com — G65/G66; G54.1/G52/G92](https://cncmacros.com/the-ultimate-guide-to-g65-and-g66-programming); [Practical Machinist — Fanuc G76 (Q-shift = параметр 5101)](https://www.practicalmachinist.com/forum/threads/fanuc-g76-boring-cycle.282543); [Machining Doctor — G54/G10/G52 offsets](https://www.machiningdoctor.com/gcodes/g54); [FANUC 0i-F Plus / 0i-C Parameter manuals (IS-B/IS-C)](https://drivesul.com.br)
