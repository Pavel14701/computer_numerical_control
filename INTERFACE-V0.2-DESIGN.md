# INTERFACE-V0.2-DESIGN — дизайн-документ разморозки API v0.2

**Версия:** v0.1 (черновик на ревью оркестратору)
**Дата:** 2026-11-22
**Статус:** ТОЛЬКО документ; код не менялся, ничего не закоммичено.
**Основание:** REPORT.md §6.3–6.4 («отложено осознанно»: #21 SrcSpan→Arc&lt;str&gt; ·
#22 AdapterOutput · Box-PR large_enum_variant · AST-флаг F-кадра) + маленькие
пункты аудита (#23-хвост legality-типизация; units в AxisLimits — уже сделано,
см. §2.6).
**Правило freeze:** изменение сигнатур — только PR к оркестратору
(eid-core/src/lib.rs, freeze v0.1). Настоящий документ — такой PR.

Все числовые оценки в документе получены измерением (см. §0), не оценкой «на
глаз». Измерительная методика и окружение зафиксированы, чтобы цифры можно
было воспроизвести.

---

## §0. Методика измерений

**Что измерялось.** Проб-крейт (temp-каталог ВНЕ workspace, workspace не
менялся), `cargo 1.88.0 / rustc 1.88.0, release, opt-level=3`, win32.

* Реальный `eid_core::ast` подключён **path-зависимостью** (читаю, не правлю) —
  «current»-цифры сняты с настоящих типов.
* Гипотетические варианты (`SrcSpanArc` = file: `Arc<str>`;
  `OpBoxed` = `Box<State>`+`Box<SrcSpan>` в 7 вариантах) — зеркальные enum'ы с
  **побайтово идентичной** структурой полей.
* Подсчёт аллокаций — counting global allocator (fetch_add на alloc/realloc);
  тайминги — best-of-5; сборка 150 000 опов (микс 85% LINEAR / 10% RAPID /
  5% ARC — типичный CAM-профиль; координаты 4dp, spindle Cw, tool Some).
* Почему 150k: `max_ops_per_program` дефолтного профиля = **200 000**
  (eid-integration/src/profile.rs, Limits::default); корпус содержал сэмплы
  2.1–2.5 MB (cap-исключения, REPORT §6.2). 150k-оповая программа — легальный,
  внутриконтрактный класс нагрузки.

**Замечание о честности.** Проб меряет фазы «построение AST» и «клон AST»
(структурную стоимость), а не полный прогон parser::parse_with_notices:
структура аллокаций у канонизаторов та же (те же Num/SrcSpan-конструкторы),
поэтому аллокационные дельты переносимы 1:1, wall-clock — с точностью до
лексера. Тайминги на этой машине зашумлены (соседние прогоны давали 80–190 ms
на одних и тех же данных) — **опорный сигнал в этом документе: счётчики
аллокаций и размеры**, они детерминированы.

### Сводка измерений (150 000 опов)

| Метрика | current | #21 Arc&lt;str&gt; | Box-PR | Box+Arc |
|---|---|---|---|---|
| `size_of::<Op>()`, байт | **592** | 584 | **480** | 480 |
| ops-Vec @150k, MB | 86.7 | 85.5 | 70.3 | 70.3 |
| сборка 150k опов: аллокации | 1 910 450 | **1 760 451 (−150k, −7.9%)** | 2 210 450 (+300k) | 2 060 450 |
| сборка: heap-байт | 108.6 MB | 105.3 MB | 111.0 MB | 107.7 MB |
| kept-pass-клон (клон каждого опа): аллокации | 1 410 001 | **1 260 001 (−150k, −10.6%)** | 1 710 001 (+300k, +21%) | 1 560 001 |
| kept-pass-клон: heap-байт | 100.3 MB | 97.3 MB | 102.7 MB | 99.7 MB |

Прочие размеры: `Num`=32, `Point3`=96, `StartPoint`=96, `State`=88,
`SrcSpan`=40 (String 24 + 4×u32 16), `SrcSpanArc`=32 (`Arc<str>` 16 + 16).
Payload-размеры вариантов Op: ARC=592, RAPID/LINEAR=328, WCS_DEFINE=232,
DWELL=168, RAW=96. `State { …, feed_restated: bool }` = **88** — bool ложится
в существующий tail-padding (см. §2.4).
Клон SrcSpan-поля (путь `op_src()` в диагностике): String → 1 аллокация ~25 ns;
`Arc<str>` → refcount, 0 аллокаций.

---

## §1. Цели и не-цели v0.2

### Цели

1. **G1 (#21).** Убрать 1 String-аллокацию на оп в `SrcSpan.file`; удешевить
   клонирование опов (verify kept-pass, Notice, диагностики). Заодно —
   **честно пересчитать заявленный «3–5× parse-speedup»** (не подтверждается;
   §2.1).
2. **G2 (#22).** Сделать все адаптеры `Sync`: убрать `RefCell`-буферы
   (notices/warnings/diagnostics) из 4 адаптеров; `Adapter::parse`/`emit`
   возвращают результат-структуру напрямую. Открывает параллельный прогон
   корпуса и многопоточный веб-сервис.
3. **G3 (Box-PR).** Закрыть вопрос `clippy::large_enum_variant` на `Op`
   **решением, а не предположением**: измерить цену/выгоду и зафиксировать
   вердикт (по измерениям — отклонить; §2.3).
4. **G4 (F-флаг).** Сохранять провенанс «F-слово было в кадре» (restated-F),
   чтобы эмиттеры воспроизводили F-кадр и rs274-ассерт `SET_FEED_RATE`
   не терялся на классе файлов e2e48-вектора #21 (SPEC §7.4a v0.18).
5. **G5 (мелкие).** (а) типизированный `LegalityCondition` вместо
   `&'static str` + устранение `panic!` на emit-пути (аудит #23-хвост);
   (б) units-поле лимитов осей — **проверить, не делать** (уже сделано, §2.6).

### Не-цели (сознательно, с основаниями)

* **N1. `Num.text: String` не трогаем.** Измерение показывает: доминирующий
  источник аллокаций — не `SrcSpan.file`, а `Num.text` (≈7–15 String на оп:
  start/end/center/normal/feed/spindle/sweep/pitch). Именно они съедают
  ~92% аллокаций сборки; замена только file-поля не может дать «3–5×».
  Типизация Num (Arc/RC-текст или f64-канонизация) — отдельная глубокая
  ревизия с влиянием на verbatim-эмиссию (SPEC §10.1) — в v0.2 не входит.
* **N2. Арена/slab-AST, индексные хэндлы вместо owned-дерева.** Единственный
  путь к честным «3–5×» (нулевые аллокации на клон), но это редизайн всего
  потребления AST (verify/diff/emit/harness) — отдельный проект, не волна
  разморозки.
* **N3. Грамматика не меняется.** Никакой serialization `feed_restated` в
  EID-текст, никакого bump grammar.ebnf / eid_version («EID/L1 v0.3»
  остаётся), goldens байт-в-байт не трогаются (§2.4, §4).
* **N4. Конфиг в trait (`Adapter::parse_with_limits`) не добавляем.** Профиль
  остаётся конвейерной обвязкой (eid-integration/translate.rs, аудит #2) —
  документированное расхождение не расширяется без отдельного решения.
* **N5. Новые диалекты (grbl-hal/fluidnc, lathe-профили) и NURBS/Q10** — вне
  v0.2 (ревью-очередь пользователя).
* **N6. Box-PR не выполняется** (это и есть решение G3: отклонено
  измерением; `#[allow]` с обоснованием остаётся).

---

## §2. По пунктам: проблема → варианты → рекомендация

### §2.1. #21 — `SrcSpan.file: String` → `Arc<str>`

#### Проблема (по коду)

`SrcSpan` (eid-core/src/ast.rs:113): `file: String`. Конструируется **по
аллокации на каждый вызов** во всех источниках AST:

| Место | Как |
|---|---|
| eid-core/src/parser.rs:664 (`src_of`), :407 | `file.to_string()` на каждый оп |
| eid-fanuc/src/canonicalize.rs:179 (`span`) | `file.to_string()` |
| eid-grbl/src/canonicalize.rs:81 (`span`) | `file.to_string()` |
| eid-linuxcnc/src/parser.rs:182 (`Ctx::src`), :242 (`fatal`), lexer.rs:100/:174 | `self.file.clone()` (String-клон) |
| eid-harness/src/generator.rs:86 (`src(i)`) | `format!("gen{i}.ngc")` **на каждый оп** |
| eid-integration/src/translate.rs:284 (`limit_diag`) | `file.to_string()` |
| тесты: roundtrip.rs (~12 литералов), spec_obligations.rs, fanuc_emit.rs:904, translate.rs-тесты | литералы `SrcSpan { file: … }` |

Потребители: `diag::format_span` (печать), `Diagnostic.src` (клон при каждой
диагностике), `raw_refusal` (клоны всех Raw-спанов), `emit_src` (печатает
**только line/col — file в EID-текст не идёт вообще**, поэтому грамматика и
goldens не затрагиваются).

**Кто клонирует горячо (verify kept-pass):** `verify::run()` клонирует каждый
оп минимум один раз — `kept.push(o.clone())` (verify.rs:615/625), плюс
`Notice { op: o.clone() }` (:621) для отброшенных; `check_linking` клонирует
`prev.end` на каждый motion-оп (:982) и origin'ы (:832/:835); `op_src()`
клонирует спан на каждую диагностику (:187). По измерению kept-pass-клон
150k-опов = 1.41M аллокаций, из них 150k — клон `file: String`.

#### Проверка заявленного «3–5× parse-speedup»

**Не подтверждается.** Арифметика по измерению:

* Сборка 150k опов = 1.91M аллокаций **всего**; `file: String` — ровно 150k из
  них (**7.9%**). Даже бесплатное устранение 8% аллокаций не даёт кратно
  ускорить parse: wall-clock-выигрыш ожидается в пределах единиц процентов
  (малые аллокации ~8-байтовых строк, malloc-доминирование).
* Доминирует `Num.text` (N1): ~12 аллокаций на оп при сборке, ~9.4 на клон.
* Единственный сценарий «3–5×» — устранение клонов как класса (арена, N2) —
  не входит в v0.2.

**Вердикт по claim:** пункт #21 правомерен как *аллокационный* (−150k на
сборку, −150k на kept-pass, −150k на полный `program.clone()` в
harness::translate), но **не как speedup-пункт**. Формулировка для
CHANGELOG/REPORT: «−8% аллокаций parse-фазы, −11% аллокаций verify-фазы на
150k-оповых программах», без «3–5×».

#### Варианты

**В1 (рекомендуется). `file: Arc<str>`.**
Паттерн: адаптер держит **один** `Arc<str>` на прогон parse (Ctx хранит
`file: Arc<str>`), `span()`/`src()` клонируют хэндл (refcount, 0 аллокаций на
оп). Все derive (`Debug/Clone/PartialEq/Eq`) сохраняются; `format_span` и
CLI-печать работают через `Display for Arc<str>`.
* Размер: `Arc<str>` (16 B) **меньше** String (24 B) — SrcSpan 40→32,
  Op 592→584 (замер §0).
* Времена жизни: диагностика может пережить Program (CLI печатает diags после
  drop программы) — Arc делает это безопасным без borrow-инверсий.
* Минус (честно): счётчик-инкремент на клоне — атомарная операция; на горячем
  kept-pass-пути это всё равно дешевле аллокации (~2 ns против ~25 ns,
  замер §0).

**В2. Индекс файла (u32) в program-level таблице.** Отклонён: `SrcSpan`
живёт **вне** Program-контекста — диагностики строятся до/без Program
(`fatal` в linuxcnc parser.rs:231 — отказ до построения Program;
`limit_diag` в integration строит SrcSpan из имени файла без Program),
golden-плейсхолдеры, хэш-таблица на каждый parse ради экономии 8 байт.
SrcSpan теряет самодостаточность — цена выше выгоды.

**В3. `&'static str`.** Отклонён: имена файлов динамические (пути корпуса,
`gen{i}.ngc`).

#### Рекомендация

**В1.** Один атомарный коммит-волна (см. §3, волна 1): тип поля + все
конструкторские точки из таблицы выше. Оценка объёма: ast.rs 1 строка +
~20 мест в src + ~20 в тестах; поведение, текст диагностики, goldens,
grammar — без изменений.

---

### §2.2. #22 — `AdapterOutput`: убрать `RefCell` (!Sync)-адаптеры

#### Проблема (по коду)

Trait v0.1 (eid-core/src/adapter.rs:6–18):

```rust
fn parse(&self, src: &str, file: &str) -> Result<Program, Vec<Diagnostic>>;
fn emit(&self, program: &Program) -> Result<String, Vec<Diagnostic>>;
```

не имеет Ok-канала для notices/warnings/diagnostics → все адаптеры держат
инterior mutability и **!Sync**:

| Адаптер | Буферы | Место |
|---|---|---|
| `EidTextAdapter` | `RefCell<Vec<Notice>>` | eid-core/src/adapter.rs:37 |
| `GrblAdapter` | notices + warnings | eid-grbl/src/lib.rs:25–26 |
| `FanucAdapter` | notices + warnings | eid-fanuc/src/lib.rs:45–46 |
| `LinuxCncAdapter` | diagnostics + notices | eid-linuxcnc/src/lib.rs:29–30 |
| (в полёте) `eid-haas` | пока без Adapter-impl — заходит сразу на v0.2 | crates/eid-haas |

Последствия:
1. `Registry { adapters: BTreeMap<String, Arc<dyn Adapter>> }`
   (eid-harness/src/runner.rs:37) — общий реестр нельзя шарить между
   потоками → параллельный прогон корпуса и per-request адаптер веб-сервиса
   заблокированы.
2. **Паттерн «parse потом take» с перезаписью буфера**: повторный `parse` на
   том же экземпляре затирает notices предыдущего (FanucAdapter::parse,
   lib.rs:136; runner R3-consistency реально переиспользует `a.parse(...)`
   на том же `Arc<dyn Adapter>`, runner.rs:395 — сегодня benign, потому что
   notices в этом пути не читаются, но инвариант держится на честном слове).
3. `AnyAdapter` в integration (translate.rs:57–135) — ручная типизированная
   надстройка с форматированием notices в `Vec<String>`, дублирующая паттерн
   «take» по всем вариантам; CLI повторяет (eid-cli/src/lib.rs:236).
4. Разнобой типов notices: core `Notice`/`NoticeKind`, fanuc `Notice`
   (7 вариантов, вкл. DpiAssumed/ModalOnRaw), grbl `Notice`
   (+NearDegenerateRArc), linuxcnc `Notice` (+MissingProgramEnd) — 4
   структуры с одинаковым скелетом `{kind, message, op_index, op}`
   (множества вариантов выписаны в В2 ниже).

#### Варианты

**В1. Ассоциированный тип `type Notice;` в trait.** Отклонён: ломает
dyn-совместимость — `Arc<dyn Adapter>` в `Registry`, `translate_checked(&dyn
Adapter, …)` (harness/translate.rs:26) и `run_r1_text(&dyn Adapter, …)`
перестают компилироваться; переезд на enum-dispatch во всех потребителях —
крупный рефактор поверх сути пункта.

**В2 (рекомендуется). Owning-результат + **единый** core-тип notice.**

```rust
// adapter.rs (v0.2)
pub struct ParseOutput {
    pub program: Program,
    pub notices: Vec<Notice>,        // ОДИН тип: core::verify::Notice
    pub diagnostics: Vec<Diagnostic>, // Ok-путь (linuxcnc RAW-узлы, fanuc DPI)
}
pub struct EmitOutput {
    pub text: String,
    pub warnings: Vec<String>,        // Q3-потери (grbl/fanuc/linuxcnc)
    pub diagnostics: Vec<Diagnostic>, // Emit-stage диагностики
}
fn parse(&self, src: &str, file: &str) -> Result<ParseOutput, Vec<Diagnostic>>;
fn emit(&self, program: &Program) -> Result<EmitOutput, Vec<Diagnostic>>;
```

Плюс supertrait-гарантия: `pub trait Adapter: Send + Sync` — после удаления
буферов все четыре адаптера тривиально Send+Sync (только данные); supertrait
делает `Arc<dyn Adapter>: Send + Sync` контрактом, а не надеждой, и даёт
compile-фейл при чьём-либо будущем interior mutability.

Ключевое решение — **расширить core `NoticeKind`** аддитивными вариантами
до супермножества вместо ассоциированного типа. Фактические множества
(проверено по коду): core = {ZeroLengthMotion, ZeroLengthArc, ZeroDwell};
grbl += {NearDegenerateRArc, MissingProgramEnd}; linuxcnc +=
{MissingProgramEnd}; fanuc += {ZeroLengthDwell (имя core-ZeroDwell),
MissingProgramEnd, NearDegenerateRArc, DpiAssumed, ModalOnRaw}. Скелет
`{kind, message, op_index, op}` у всех четырёх идентичен (canonicalize.rs:107
fanuc, :41 grbl, parser.rs:60 linuxcnc, verify.rs:152 core), потери
информации нет, dyn-совместимость сохранена; расходятся только имена
zero-dwell-варианта (ZeroDwell ↔ ZeroLengthDwell — унифицируется на core-имя
с алиасом при конверсии). Диалектные структуры `Notice` остаются внутренними
и конвертируют на границе адаптера (From-impl).

**В3. Эрез в строки на границе trait (`notices: Vec<String>`).** Возможен как
fallback, отклонён как основной: harness/diff-тесты и будущие потребители
(например, отчёт «какие опы отброшены») теряют машиночитаемый kind/op_index;
`AnyAdapter::take_notices` уже показывает, что строковое форматирование —
слоёная политика, а не контракт.

#### Что это открывает (обоснование выгоды)

* `Arc<dyn Adapter>: Send + Sync` → `Registry` шарится между потоками;
  harness `run()` может rayon-ить по векторам манифеста (сегодня — цикл,
  runner.rs:128).
* Веб-сервис: один адаптер на поток без `new()` на запрос; либо `new()` на
  запрос остаётся дешёвым (адаптеры уже только-данные).
* Устраняется класс «затёртых notices» (п.2 выше): результат parse —
  owning-значение, порядок потребления не важен.

#### Совместимость и миграция (детально — §3, волна 2)

Call-sites по потребителям (полный обход):
* eid-core: adapter.rs (EidTextAdapter::parse/emit), тесты roundtrip.rs:391–400
  (`take_notices`-паттерн → `out.notices`).
* eid-grbl: lib.rs (Adapter-impl, take_notices/take_warnings → удаление),
  тесты grbl_emit.rs (emit-предупреждения → EmitOutput.warnings).
* eid-linuxcnc: lib.rs (Adapter-impl, take_diagnostics/take_notices),
  тесты linuxcnc_emit.rs:572+ (SET_FEED_RATE-тесты — на EmitOutput).
* eid-fanuc: lib.rs (Adapter-impl + take_notices/take_warnings), fanuc_emit.rs.
* eid-harness: runner.rs (6+ мест `adapter.parse(...)`, `run_golden`
  `eid.parse/emit` + `take_notices`, `run_r1_text`), translate.rs
  (`translate_checked`, `Ok((program.clone(), text))` → `(ParseOutput,
  EmitOutput)`).
* eid-integration: translate.rs `AnyAdapter` — **упрощается** (исчезает
  take_notices/take_emit_diagnostics/take_q3_warnings-троица);
  `TranslateOutcome.notices` → `Vec<Notice>` (типизированный; строковый
  формат для печати — `Display for Notice`, формат строк сохраняется:
  `{kind:?}: {message} (op {op_index})`).
* eid-cli: lib.rs:226–236 (`adapter.parse` + `take_notices` → `out.notices`).
* fuzz-таргеты (fuzz/fuzz_targets/*.rs): вызовы parse → новая сигнатура.

`take_*`-методы: удалить в той же волне (один breaking-раз, freeze-дисциплина
«одна правка сигнатур — один PR»); совместимые шлюзы не нужны — все
потребители внутри workspace (внешних потребителей crates нет; см. §4).

---

### §2.3. Box-PR large_enum_variant (`Op`)

#### Проблема

`clippy::large_enum_variant` предлагает `Box<State>`/`Box<SrcSpan>` в 7
вариантах `Op` (eid-core/src/ast.rs:122–130 — allow с комментарием-историей:
подсказка clippy = compile-break трёх адаптеров + harness, заморожено).
Цена/выгода проверена измерением (§0).

#### Измерение

| | current | Box-PR |
|---|---|---|
| `size_of::<Op>()` | 592 B | 480 B (−112) |
| ops-Vec @150k | 86.7 MB | 70.3 MB (**−19%**) |
| аллокации сборки | 1 910 450 | 2 210 450 (**+300k, +16%**) |
| аллокации kept-pass-клона | 1 410 001 | 1 710 001 (**+300k, +21%**) |
| аллокации program.clone() | 1 410 001 | 1 710 001 (+300k) |
| clippy-зазор (largest − 2nd) | 592−328 = **264** > 200 | 480−216 = **264** > 200 |

Два решающих факта:
1. **Клоны дорожают.** `Op::clone` — самая горячая операция AST (kept-pass
   клонирует каждый оп; Notice — тоже; translate — `program.clone()`).
   Box добавляет 2 аллокации на клон опа (+21% аллокаций на самой горячей
   фазе) ради экономии на **move**-ах (Vec-рост, передача по значению),
   которые в текущем пайплайне редки (опы строятся один раз и живут в Vec).
2. **Линт всё равно не гаснет.** Зазор ARC-варианта (3 лишних `Point3` =
   96×3 = 288 B) доминирует: после Box-PR 480−216 = 264 > 200 →
   `#[allow]` остаётся в любом случае.

#### Варианты

* **В1 (рекомендуется): статус-кво** — точечный `#[allow(clippy::
  large_enum_variant)]` с обоснованием (уже есть, ast.rs:122–129),
  дополнить комментарий цифрами из этого документа.
* **В2: Box-PR по подсказке clippy.** Отклонён: +300k аллокаций на фазах
  сборки/клона, −19% только на размере Vec, линт не устраняет.
* **В3: глубокая переформа (Box только в ARC-варианте, или
  `Box<[Num;3]>`-геометрия).** Отклонена для v0.2: трогает те же горячие
  клоны; шанс вернуться — только с доказанным memory-профилем (реальная
  200k-оповая программа, RSS-лимиты), как отдельный измеренный PR.

#### Связка с #21

`Arc<str>` уменьшает SrcSpan 40→32 ⇒ Op 592→584 — то есть #21 **почти не**
уменьшает вариант (−8 B). Пункты ортогональны: #21 — про аллокации, Box — про
размещение. Выполнять #21 (безвредно, аллокации ↓), Box-PR — отклонить.

#### Рекомендация

**В1** + дописать измеренные цифры в комментарий allow. В чек-лист приёмки —
пункт «clippy -D warnings зелёный с allow на месте» (§5).

---

### §2.4. AST-флаг F-кадра (restated-F, хвост v0.18)

#### Проблема

SPEC §7.4a v0.18 (зонд-подтверждено, probe `g81_nof`): **rs274 ассертит
`SET_FEED_RATE` на каждом кадре с F-словом**. Наш AST несёт `state.feed` как
модальный снапшот — **restated-F** (F-слово в кадре при неизменном значении)
неотличим от наследованного feed. Эмиттеры излучают F только при смене
значения или на первом производном опе цикла (`sub == Some(1)`):

* eid-linuxcnc/src/emit.rs:671–704 (`feed_word`: `changed || first_derived`);
* eid-fanuc/src/emit.rs:798–835 (зеркало);
* eid-grbl/src/emit.rs:457–465 (F-дельта в emit_op, то же правило).

Эмиссионное следствие: файл с повторным `F120` на каждом кадре после
трансляции теряет повторные F → в rs274-каноне перевода нет `SET_FEED_RATE`
на этих кадрах → расхождение канонических потоков. Это класс красного
e2e48-вектора #21 («−restated-F по ruling b», REPORT §6.3); сегодня красного
нет только потому, что корпус не содержит restated-F файлов (весь корпус
циклов несёт явное F по одному на значение). Дыра открыта.

#### Варианты

**В1 (рекомендуется): поле в State-снапшоте — `feed_restated: bool`.**
* Измерение: `State` 88 B **не растёт** (bool — в существующий tail-padding);
  `Op` остаётся 592 B; клонирование — +0 аллокаций.
* Семантика: провенанс «F-слово присутствовало в кадре-источнике этого опа».
  Все кадры, которые могут излучить F, несут State (GRBL излучает F даже на
  rapid-кадре; dwell/tool_change/stop идут с `pre_state`-модалью) — поле
  автоматически доступно всем армам.
* Правило эмиссии во всех трёх feed_word: `changed || first_derived ||
  state.feed_restated`.
* Продюсеры: fanuc canonicalize — точка абсорбции `b.word('F')`
  (canonicalize.rs:1384) ставит `ctx.feed_restated = true` (сброс на блоке);
  grbl/linuxcnc — симметричные точки абсорбции F; **G93-разрешение**
  (`st.feed = len·F`, fanuc:1861/:2124, grbl:723) — restated = true (F-слово
  было в кадре; маркер кадра важен независимо от преобразования значения);
  eid-core parser и harness generator — `false`.
  * Нюанс развёртки циклов: restated=true несёт **только** оп,
    соответствующий кадру-источнику с F-словом (первый производный, sub=1 —
    согласуется с уже принятым маркером `sub == Some(1)`); производные опы
    2..n — false (их «кадров» в источнике нет, лишний F на каждом сломал бы
    счёт SET_FEED_RATE в rs274-каноне).
* Совместимость с оракулами:
  * **semantic_diff исключает поле** (provenance-класс, как `src`/`sub` —
    это явно задокументированный класс «вне эквивалентности», diff.rs:77–79,
    EQUIVALENCE §5): иначе R3-golden краснеет (диалектный parse ставит true,
    golden — false).
  * `emit_state` (eid-core/emitter.rs:76–105) **не сериализует** поле ⇒
    R1/R2 байт-в-байт не меняются, grammar.ebnf не меняется, **goldens не
    регенерируются**, eid_version остаётся «EID/L1 v0.3». Потеря флага через
    eid-l1 текстовый мост = потеря провенанса класса src/sub — принятая
    граница уровней (CORPUS lossy-класс), задокументировать в EQUIVALENCE §5.
* Стоимость миграции: 45 литералов `State {` в 19 файлах (grep, workspace) —
  компилятор enforce (структурный литерал требует все поля) → чек-лист
  компилятором же закрывается.

**В2: флаг на Op (как `sub`) в 4–7 вариантах.** Точечнее по объявлению, но:
(а) те же точки-потребители (армам emit'а нужен флаг на каждом
State-несущем варианте — rapid/linear/arc/dwell/tool_change/stop/wcs_*),
(б) churn по всем exhaustive match'ам (verify.rs verify_op/verify_kept_op —
компилятор enforce, emitter.rs emit_op, diff.rs) **поверх** неизбежного
churn по конструкторам, (в) +1 байт на вариант с риском +8 на выравнивание в
RAPID/LINEAR (Option&lt;u32&gt;-хвост). Не даёт ничего сверх В1.

**В3: collapse дублей `SET_FEED_RATE` в rs274-компараторе.** Отвергнут в
v0.18 («честнее 1:1», ruling по probe `g81_nof`) — **отвергается снова**, с
явными основаниями: (1) компаратор — эквивалентностный оракул; collapse
маскирует именно тот сигнал (счёт ассертов), которым мы ловим потерю
F-кадров; (2) не чинит саму вербатим-потерю: переведённый G-код реально не
содержит F-маркер кадра → любой повторный parse/потребитель (не только
rs274) видит другой кадр; (3) прецедент «правим оракул под реализацию»
ломает дисциплину зонд-сверки (SPEC v0.18 принят противоположным решением).

#### Рекомендация

**В1.** Волна 3 (§3): поле + продюсеры + три feed_word + diff-исключение +
аддитивные корпус-векторы (restated-F fanuc/linuxcnc + rs274-вектор) в одном
атомарном коммите с diff.rs (иначе R3-окно красное).

---

### §2.5. Мелкий пункт: типизированный `LegalityCondition` (аудит #23-хвост)

#### Проблема / текущий статус

**Частично сделано**: ConversionTarget-паттерн внедрён во все три цели —
таблицы легальности как данные (fanuc lib.rs:101–114 `FanucEmissionLegality`,
grbl lib.rs:49–91, linuxcnc lib.rs:51–80), код отказа берётся только из
таблицы («единый путь отказа»). Остаток хвоста: ключи — `&'static str`, поиск
— `code_of(&str) -> Option<ErrorCode>`, а вызовы — с **паникой** на
необъявленное условие:

* fanuc emit.rs: `panic!`-ветка `validate_fanuc` (:317) + `.expect`
  ×3 (:452, :468, writer :557);
* grbl emit.rs: `panic!`-ветка (:292) + `.expect` ×2 (:392, :401)
  (+ отдельный `.expect("нормаль провалидирована")` :483 — вне legality,
  не в объёме этого пункта);
* linuxcnc emit.rs: `panic!`-замыкание `code()` (:131).

Т.е. на emit-пути пользователя сидит класс panic «таблица и эмиттер
разошлись» — сегодня недостижим (legality_behavior_cross_check в тестах), но
неустраним компилятором: новое условие в эмиттере без строки в таблице = рантайм-
паника, не ошибка сборки.

#### Варианты

* **В1 (рекомендуется): enum `LegalityCondition`** (по одной цели: `Fanuc…`,
  `Grbl…`, `LinuxCnc…` — варианты не пересекаются: у fanuc есть
  `WcsDefinePOutside…`, у linuxcnc `ArcUnknownStart = RuntimeState`-класс):
  таблица `Vec<(LegalityCondition, ErrorCode)>`, `code_of(cond) -> ErrorCode`
  — **тотальная** функция (match без Option), `as_str()` сохраняет
  канонические имена (`"wcs_set_here"` — их читают тесты fanuc_emit.rs:588–656
  и COVERAGE §5.1; manifest r4_expect хранит КОДЫ, не имена условий —
  корпус не затрагивается). Тесты переходят с `code_of("строка")` на
  `code_of(Cond::X)` (~12 мест в fanuc_emit.rs, аналогично grbl).
* **В2: оставить строки, заменить `.expect` на fallback-код.** Отклонён:
  дрейф таблицы начинает молча менять код отказа — хуже паники (тихая
  неправильность vs громкая); статическая гарантия не появляется.

#### Рекомендация

**В1**, волна 4 (низкий приоритет, изолированные файлы). Побочный эффект:
мутационное тестирование (mutants.out) получает компилятор-защищённую
таблицу — меньше мутантов класса «убрал строку из таблицы».

### §2.6. Мелкий пункт: units в AxisLimits — УЖЕ СДЕЛАНО, не дублировать

Проверено: `MachineProfile.axis_limits_units: Units` с serde-дефолтом mm
существует (eid-integration/src/profile.rs:196, `axis_limits_units`, модуль
`units_serde`, #27) — сделано агентом E в профиле; поле НЕ внутри
`AxisLimits` (struct :123), а рядом в профиле, что соответствует решению.
В v0.2 — только приёмочная проверка (profile_matrix зелёный), изменений нет.

---

## §3. Миграционный план

### Порядок волн (зависимости: core → адаптеры → потребители)

| Волна | Содержание | Бласт-радиус | Совместимость |
|---|---|---|---|
| **0. Документы** | SPEC: §7.4a-дополнение (AST несёт restated-провенанс, правило эмиссии v0.18 обобщается: `changed ∨ first_derived ∨ restated`); EQUIVALENCE §5: `feed_restated` в списке «вне эквивалентности» (провенанс); COVERAGE: план restated-F векторов; этот doc — approved | только .md | — |
| **1. #21 Arc&lt;str&gt;** | ast.rs (1 тип), parser.rs (2), fanuc/grbl span(), linuxcnc src()/fatal()/lexer×2, generator.rs, integration limit_diag, тесты (~20 литералов) | 5 крейтов + тесты, **без** verify/emitter/diff (Clone/derive) | goldens/R1–R4/grammar: без изменений; вых. текст диагностики без изменений |
| **2. #22 AdapterOutput** | adapter.rs (ParseOutput/EmitOutput + NoticeKind-супермножество + `trait Adapter: Send + Sync`), 4 Adapter-impl, harness runner/translate, integration AnyAdapter (упрощение), CLI, fuzz-таргеты; `take_*` удаляются; **compile-canary**: тест `fn assert_sync<T: Sync>() {}` × 4 адаптера | 7 крейтов (один breaking-раз) | текст notice-строк сохранён (Display); goldens/R1–R4 без изменений; поведение «затёртых notices» легализовано (результат owning) |
| **3. F-флаг (В1)** | ast.rs State+1 поле (атомарно с diff.rs!), 3 канонизатора (точки абсорбции F, вкл. G93-ветки), 3 feed_word, diff.rs исключение + eq4-тест, **аддитивные** корпус-векторы restated-F + rs274-вектор, manifest version-bump (additive) | 6 крейтов + corpus (только добавление) | goldens существующие: 0 diff; grammar/eid_version: без изменений; существующие 48 rs274-векторов: зелёные (у них флаг false) |
| **4. LegalityCondition** | 3 emit.rs (enum + тотальный code_of), тесты fanuc/grbl (~15 мест) | 3 крейта, изолированно | коды отказов имена условий: 1:1; manifest/COVERAGE: без изменений |
| **—. Box-PR** | не выполняется; комментарий allow дополняется цифрами §2.3 | ast.rs:122–129 комментарий | — |

Параллельность волн: 1 и 2 независимы (могут идти параллельно разными
агентами при reb’ейзе core-первым); 3 — строго после 2 (один и тот же
файл adapter-сигнатур не пересекается, но корпус/manifest правки
складывать поэтапно); 4 — в любой момент после 2.

### Зоны агентов

* **Агент A (eid-core):** волны 1 (ast/parser), 2 (adapter.rs + EidTextAdapter),
  3 (ast.rs State + emitter — правило в core-эмиссии не меняется, т.к.
  `emit_state` не трогаем; только тип), 4 (нет). Держатель freeze-PR.
* **Агент B (eid-harness):** волна 1 (generator.rs), 2 (runner/translate:
  ~10 call-sites, run_golden, run_r1_text), 3 (дифф-исключение в core
  делает A, но eq4-тесты + новые векторы + manifest — B; rs274_compare без
  изменений — collapse не делаем), приёмочный параллельный прогон (demo,
  не обязательный для зелёности).
* **Агент C (eid-grbl, eid-fanuc, eid-haas):** волна 1 (два span()), 2 (два
  Adapter-impl + удаление take_*), 3 (точки абсорбции F в двух
  канонизаторах + два feed_word), 4 (два emit.rs + тесты). eid-haas — в
  полёте: **сразу** писать на v0.2-сигнатурах (волна 2), чтобы не
  мигрировать дважды; его 3 литерала `State {` входят в зачистку волны 3.
* **Агент D (eid-linuxcnc):** волна 1 (src/fatal/lexer), 2 (Adapter-impl),
  3 (абсорбция F + feed_word), 4 (emit.rs enum).
* **Агент E (eid-integration, eid-cli):** волна 1 (limit_diag), 2
  (AnyAdapter-упрощение + TranslateOutcome: типизированные notices/warnings;
  CLI), приёмка units (§2.6 — уже зелёный).

Порядок merge в каждой волне: core-коммит → адаптеры (C, D параллельно) →
harness/integration (B, E) → workspace-зелёный gate → CHANGELOG/REPORT.

### Совместимость (контрольный список поверхностей)

* **4 адаптера + haas:** сигнатуры parse/emit меняются один раз (волна 2);
  haas-скелет (lexer/state) — без Adapter-impl, безопасен.
* **harness:** runner.rs parse-сайты, run_golden, run_r1_text, R3-consistency
  re-parse; Registry (Arc&lt;dyn Adapter&gt;) — тип не меняется (В2 сохраняет
  dyn). generator.rs — 1 точка. mutate.rs — 3 литерала State (волна 3).
* **integration:** AnyAdapter (сокращается), TranslateOutcome (поля notices/
  emit_warnings/q3_warnings → типизированные; строковые форматы печати —
  сохранены), sim/profile-тесты — без изменений.
* **fanuc-контракт (fanuc_dialect_analysis.md §8):** не затрагивается —
  волна 4 меняет внутренний ключ таблицы при сохранении строк-имён
  (`as_str()`), legality-поведение и коды 1:1.
* **CLI:** печать notice — формат строк сохранён; verify-конвейер — только
  вызовы.
* **Внешние потребители:** внешнего API нет (workspace-crates не публикуются,
  Cargo.lock без внешних зависимостей; CLI-бинарник — единственный наружный
  интерфейс, его stdout-форматы зафиксированы выше).

---

## §4. Регистр рисков

| # | Риск | Волна | Вероятность / влияние | Митигация |
|---|---|---|---|---|
| R1 | **Goldens.** R1/R2 байт-в-байт красный, если кто-то сериализует `feed_restated` в `emit_state` или тронет emit_op-текст | 3 | низкая / высокое | Запрет в дизайне (N3): поле — провенанс, не текст; приёмка «git diff corpus/ пуст» (§5); emit_state не изменяется |
| R2 | **R3-golden красный** при атомарности: диалектные парсы ставят флаг, golden — нет, diff ещё сравнивает | 3 | высокая без митига / среднее | diff.rs-исключение **в том же коммите**, что поле; eq4-тест на «restated=true vs false ≡ эквивалентны» |
| R3 | **R3-consistency / R1-text / R4** — ложные красные от смены сигнатур (нечаянная потеря notices-семантики) | 2 | низкая / среднее | Рефактор без изменения правил; runner-тесты двух классов негативов остаются; приёмка: полные счётчики §5 |
| R4 | **rs274/e2e48.** Новое правило F-эмиссии меняет текст на restated-файлах: существующие 48 векторов должны остаться 1:1 | 3 | средняя / высокое | Флаг false на всех существующих конструктах (циклы дают sub=1-маркер, значение restated не меняют); аддитивные векторы — отдельно; существующие rs274-прогоны в приёмке |
| R5 | **Поведенческая дельта #22:** раньше повторный parse затирал notices буфера; теперь каждый parse возвращает свои | 2 | низкая / низкое | Это фикс, не регресс; тест roundtrip.rs:391–400 переписывается на owning-семантику |
| R6 | **Fuzz-таргеты** сломаны сменой сигнатур → CI-красный после волны 2 | 2 | высокая / низкое | fuzz/fuzz_targets/*.rs входят в волну 2 (механика: match на ParseOutput) |
| R7 | **clippy-гейт:** удаление/ослабление allow на Op → волна красного CI | все | средняя / низкое | Box-PR отклонён; allow остаётся с цифрами; чек-лист `-D warnings` |
| R8 | **Тайминги-ожидания:** stakeholder ждёт «3–5×» от #21, получает единицы процентов | 1 | — / репутация | Claim-ревизия в этом документе (§2.1): обещаем −8%/−11% аллокаций, не speedup; bench-числа в REPORT |
| R9 | **eid-haas в полёте** — мержится в середину волн, конфликт по State-литералам и Adapter-impl | 2–3 | средняя / среднее | Haas сразу на v0.2; зона C координирует; зачистка State-литералов включает haas |
| R10 | **Параллельный прогон корпуса** (новая возможность #22) вскрывает генерацию .out-коллизий rs274 (#28 гард) в многопотоке | 2 (опционально) | низкая / среднее | Уникальность уже pid+seq (rs274_compare.rs:99–104) — потокобезопасно; параллельный запуск — demo, не приёмочный гейт |
| R11 | **Двойная потеря провенанса через eid-l1 мост** (restated-F теряется в fanuc→eid-l1→fanuc) | 3 | детерминировано / низкое | Задокументировано как lossy-класс src/sub (EQUIVALENCE §5); same-dialect fanuc→fanuc идёт через AST в памяти — флаг сохраняется |
| R12 | **Откат.** Любая волна ломает больше, чем чинит | все | — | Волны независимы, каждая — серия коммитов без data-/format-миграций; corpus — только additive; git revert по волнам |

---

## §5. Чек-лист приёмки v0.2

Базовые счётчики (текущий REPORT): 758 тестов · R1 48/0 · R2 48/0 ·
R3-golden 83/0 · R3-consistency 35/0 · R1-text 8/0 · R4 8/0 · Tier B
144 crash=0 · rs274_e2e48 0 divergences.

- [ ] **T1. Workspace зелёный:** `cargo test --workspace` — 0 failed
      (счётчик ≥ 758 + новые).
- [ ] **T2. Goldens нетронуты:** `git diff corpus/` пуст после волн 1, 2, 4;
      в волне 3 — только аддитивные restated-F векторы + manifest
      version-bump (additive).
- [ ] **T3. Оракулы:** R1 48/0 · R2 48/0 · R3-golden 83/0 · R3-consistency
      35/0 · R1-text 8/0 · R4 8/0 · Tier B 144 crash=0.
- [ ] **T4. rs274:** все существующие e2e48-векторы 0 divergences **плюс**
      новый restated-F вектор: SET_FEED_RATE воспроизводится на каждом
      F-кадре (fanuc и linuxcnc поверхности).
- [ ] **T5. Sync-canary (волна 2):** `trait Adapter: Send + Sync` в сигнатуре;
      compile-time assert `EidTextAdapter: Sync`, `GrblAdapter: Sync`,
      `FanucAdapter: Sync`, `LinuxCncAdapter: Sync` (+ `Arc<dyn Adapter>:
      Send + Sync` в runner-тесте); grep-гейт: `RefCell` отсутствует в
      `crates/*/src` (кроме легитимных не-адаптерных использований, если
      останутся — по списку в PR).
- [ ] **T6. Грейферы:** `SrcSpan.file` — `Arc<str>`; в emit-пути нет
      `.expect("emission-legality`/`panic!("emission-legality` (волна 4);
      `take_notices`/`take_warnings` удалены из pub-API адаптеров.
- [ ] **T7. Аллокационный бюджет** (проб §0, тот же микс 150k): сборка AST
      ≤ 1.80M аллокаций (v0.1: 1.91M), kept-pass-клон ≤ 1.30M (v0.1:
      1.41M); числа зафиксированы в REPORT (claim-ревизия: без «3–5×»).
- [ ] **T8. F-флаг семантика:** unit-тесты: (а) restated-F с тем же
      значением → F излучён; (б) наследованный feed (нет F-слова) → F не
      излучён; (в) G93-разрешённый feed → F излучён; (г) semantic_diff
      (restated=true) vs (restated=false) = пусто.
- [ ] **T9. Legality-типизация:** `code_of` — тотальный (нет Option) во всех
      трёх целях; legality_table_composition / legality_behavior_cross_check
      зелёные на enum-вариантах.
- [ ] **T10. CI:** fmt + clippy `-D warnings` зелёные (allow на `Op` на
      месте, комментарий дополнен цифрами §2.3); fuzz-таргеты компилируются
      и проходят smoke.
- [ ] **T11. Документы:** SPEC (§7.4a-правило + State-провенанс),
      EQUIVALENCE §5 (+feed_restated), COVERAGE (новые векторы, закрывающие
      записи), CORPUS-CONTRACT (manifest version), CHANGELOG (по волне),
      REPORT (измерения §0 + вердикты §2); этот документ — отмечен
      approved/отклонён по пунктам оркестратором.
- [ ] **T12. Не-сделано осознанно (негативные приёмки):** Box-PR отсутствует;
      `Num.text` не изменён; grammar.ebnf/eid_version без bump; профильные
      лимиты вне trait; units в AxisLimits — без новых правок (проверка T1
      покрывает).

---

## Приложение А. Точки абсорбции F-слова (для волны 3, по grep)

| Крейт | Точка | Что делать |
|---|---|---|
| eid-fanuc | canonicalize.rs:1384 `if let Some(f) = b.word('F')` | `ctx.feed_restated = true` (сброс при потреблении блока); `state_with_frame` (:336) переносит в снапшот; **в развёртке цикла — только первый производный оп (sub=1), остальные false** |
| eid-fanuc | G93-ветки :1861, :2124 (`st.feed = len·F`) | restated = true (F-слово было) |
| eid-grbl | canonicalize.rs:206/229 (`feed: Option<Num>` в Ctx, снапшот) | аналогичная пара поле→снапшот (+ nuance развёртки) |
| eid-linuxcnc | parser.rs:143/165 (`feed: Option<Num>`, `state_of`) | аналогично |
| eid-core | parser.rs `state()` (EID-текст) | всегда false (провенанс не сериализуется) |
| eid-harness | generator.rs `state()` | всегда false |

## Приложение Б. Полнота обхода call-sites волны 2 (по grep `\.parse(`, `\.emit(`, `take_`)

eid-core: adapter.rs (impl), tests/roundtrip.rs:391–400 · eid-grbl: lib.rs:105–117,
tests/grbl_emit.rs · eid-linuxcnc: lib.rs:100–111, tests/linuxcnc_emit.rs ·
eid-fanuc: lib.rs:134–149, tests/fanuc_emit.rs · eid-harness: runner.rs:246,
395, 642, 667, 691 (run_golden), run_r1_text:735+, translate.rs:32/62 ·
eid-integration: translate.rs:142–148, 462–488, 536 · eid-cli: lib.rs:226–236 ·
fuzz: fuzz_eid_core_parse.rs, fuzz_grbl_parse.rs, fuzz_linuxcnc_parse.rs.
