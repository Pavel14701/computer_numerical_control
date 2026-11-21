# HAAS-DIALECT-ANALYSIS — исследование Haas-диалекта (NGC mill) для COVERAGE-матрицы и планирования адаптера

**Версия:** v0.0 (research-проход)
**Привязан к:** SPEC-EID-L1 **v0.16**, COVERAGE **v0.8**, EQUIVALENCE **v0.3.1**, grammar.ebnf **v0.3**; формат и терминология — по образцу `fanuc_dialect_analysis.md` v0.0 (2-й диалект этапа 10-подготовки)
**Статус:** информационный документ этапа 10-подготовки; диспозиции подлежат приёмке по правилам COVERAGE §0 (100% словаря) и ревью. `?` — несверенное (реестр — §10): часть официальных страниц haascnc.com недоступна из research-среды (Cloudflare); сверено по официальным спискам G/M-кодов, страницам Haas UK, агрегаторам и перекрёстно.
**Скоуп:** Haas **mill** (VF/ST-вертикальные, mini-mill, DT/DM и пр.) на **NGC** (Next Generation Control). Classic Haas Control (CHC) — только там, где поведение коллидрует (помечено). Токарные (lathe) коды — только при коллизии с фрезерными (предупреждение адаптеру).

---

## 0. Резюме и правила матрицы

### 0.1 Главная находка

Статическое CAM-подмножество Haas **почти совпадает с Fanuc/LinuxCNC** (и потому с L1): движение G0–G3, плоскости, G90/G91, G94, единицы, G54–G59, G10 L2 P1–P6, G53, dwell, стопы/концы, T+M06, СОЖ — маппятся 1:1. Но гипотезы, данные в постановке задачи, **частично опровергнуты** — документ фиксирует фактическое состояние:

1. **Ловушка №1 (DPI) — направление противоположно гипотезе.** Haas по умолчанию — **«machinists notation»** (аналог Fanuc DPI=0), а НЕ calculator: `X25` в mm-программе = **0.025 mm** (25 × 0.001 mm), в inch = 0.0025". Calculator-режим — это **Setting 162 «Default To Float»**, дефолт **OFF**; отдельный **Setting 77 «Scale Integer F»** (дефолт OFF: `F12` = 0.0012 units/min). Least input increment — **0.0001"/0.001 mm** (совпадает с Fanuc IS-B). Трейлинг-точка (`X25.`) легальна и обязательна в эмиссии. Fanuc-политика §9.1(1) переиспользуется as-is.
2. **Расширенный WCS:** G110–G129 = слоты **#7–#26** = алиасы **G154 P1–P20**; всего G154 P1–P99 поверх G54–G59. **G54.1 и G59.1/.2/.3 в словаре Haas отсутствуют** (N/A). **G10 L20 на Haas = запись origin доп. СК (G110–G129/G154)** — та же ловушка, что Fanuc L20: маппить на wcs_set_here **запрещено**. **G10 L2 P0 на Haas = запись слота G52** (не «external offset» Fanuc).
3. **G52 → ERR(E_UNSUPPORTED_SEMANTICS)** — как в Fanuc-матрице: позиционно-зависимая смена фрейма mid-program. Haas-усложнение: G52/G92 **делят общий регистр сдвига**, интерпретация зависит от **Setting 33 (FANUC/HAAS)**, очистка на M30 — **Setting 129** (`?` деталей).
4. **G61/G64 — семейство exact-stop** (группа 15): G61 = exact stop mode, **G64 = cancel G61** (без квантификатора t!). Значения path_mode сдвинуты vs LinuxCNC (G61 там exact_path, G64 blend(t)); blend → Haas = bare-дефолт/G64, t непредставим.
5. **Коллизии адресов с Fanuc:** G70/G71/G72 на Haas = **болтовые окружности/дуги/линейки** (НЕ units-псевдонимы G20/G21, как у Fanuc mill, и НЕ токарные циклы); **K** в циклах = min-пек (G83)/угловой шаг (G12), повтор цикла — словом **L**; **G77** = back bore (G87/G88 в словаре Haas отсутствуют); **G76** — fine boring, направление сдвига Q задаётся **I/J-словами в тексте** (не параметром станка, как у Fanuc 5101).
6. **Canned cycles Haas** разворачиваются по Haas-семантике (§5): G73 retract-δ = **Setting 22** (дефолт 0.02"/0.5 mm — **совпадает с канон-константой SPEC §7.4a**), G83 — полный отвод (+Setting 52 «retract above R»), G86 — рестарт шпинделя подтверждён официально, G84 — rigid внутренне (Setting 57), **M29 на Haas = output relay, НЕ rigid-tap** (коллизия с Fanuc!), повторы G91-циклов — L.
7. **Haas — живой диалект Macro B** (Fanuc-совместимый: G65/G66/G67, `#`-переменные, IF/GOTO/WHILE, M97/M98/M99) + собственные extensions (NGC #10000s/#109xxx, VPS). По SPEC §1.1/v0.16 — line-level `Raw{E_UNSUPPORTED_SYNTAX}`, декомпозиция запрещена (L2, этап 12).
8. **M13/M14-гипотеза опровергнута:** на Haas mill M12/M13 = engage/release **5-й оси тормоз**; M10/M11 = 4-й оси. «Шпиндель+СОЖ»-комбинации — generic Fanuc-идиома, в словаре Haas mill отсутствуют. Вендорское ведро M-кодов широкое (двери, конвейер, TSC, PulseJet, паллеты, probing-сервис M75, интерактивный ввод M109).

### 0.2 Терминология диспозиций (совместима с COVERAGE §1.3)

| Метка | Значение |
|---|---|
| `NODE(n)` | узел SPEC §5 (rapid/linear/arc/dwell/tool_change/program_stop/program_end/wcs_define/wcs_set_here/raw) или развёртка §7 |
| `ABSORB` | разрешается в поля state / метаданные (§4 SPEC) |
| `META` | метаданные программы (units, path_mode, frame_preset) |
| `IGNORED_SAFE` | безопасный drop (узкий класс: N, `%`, G80) |
| `RAW(code)` | line-level raw с диагностическим кодом; same-dialect round-trip байт-в-байт, кросс-диалект — `E_RAW_UNTRANSLATABLE` |
| `ERR(code)` | **жёсткий отказ** (не raw) — только класс SPEC §8 правило 5: raw-сохранение исказило бы интерпретацию последующих опов (смена фрейма mid-program, смена единиц, структура программы) |
| `N/A` | код отсутствует в словаре |

Коды: `E_INVALID_INPUT` / `E_UNSUPPORTED_SYNTAX` / `E_UNSUPPORTED_SEMANTICS` / `E_REQUIRES_MACHINE_TABLE` / `E_RUNTIME_STATE` / `E_LIMIT_EXCEEDED` / `E_RAW_UNTRANSLATABLE` (COVERAGE §1.1).

### 0.3 Ограничения применимости

- **NGC vs CHC:** списки ниже — NGC (официальные страницы «Mill — G-Codes»/«Mill — M-Codes», haascnc.com). На CHC словарь уже (нет G150/G167/G174/G184/G234/G25x/G26x; G187 имела P1–P4-форму `?`). Валидация по конкретному станку — профиль этапа 8 (COVERAGE §1.2).
- **Setting-зависимость:** часть семантики Haas управляется Setting (22, 33, 40, 52, 57, 77, 129, 162, 58…) — числа настроек **вне текста программы**: классифицировать как machine-table/runtime-класс, аналогично параметрам Fanuc (5100-серия).
- Диспозиции даны для «полного» NGC mill-профиля; опции (Macro, rotation/scaling, probing, TSC, 4/5-оси, Cylindrical Mapping) отмечены. Всё несверенное — `?` (§10).

---

## 1. Профиль диалекта (что обязан знать адаптер)

### 1.1 Модальные группы (NGC mill, по официальному списку)

| Группа | Коды | Смысл | L1-рельеф |
|---|---|---|---|
| 00 (one-shot) | G04, G09, G10, G12, G13, G28, G29, G31, G35, G36, G37, G47, G52 (00-форма `?`), G53, G60, G65, G100, G101, G103, G107, G136, G150, G167, G174, G184, G187, G253, G266 | действуют один блок | dwell, G10-записи, G53, home, probing |
| 01 | G00, G01, G02, G03 | движение | NODE(rapid/linear/arc) |
| 02 | G17, G18, G19 | плоскость | ABSORB |
| 03 | G90, G91 | abs/inc | ABSORB |
| 04 | — (**отсутствует!**) | центр дуги: **G90.1/G91.1 в словаре нет** — I/J/K **всегда инкрементальные от start** (официальный ман: «I J K values define the relative X Y Z distances from the starting point») | как GRBL; ABSORB-канон §6 SPEC без режима |
| 05 | G93, G94, G95 | inverse-time / per-min / per-rev | ABSORB (G93); G95 RAW (§2.2) |
| 06 | G20, G21 | inch / mm | META(units); **псевдонимов G70/G71 нет** (заняты bolt-паттернами) |
| 07 | G40, G41, G42, G141 | 2D/3D+ cutter comp | RAW-семейство G40 |
| 08 | G43, G44, G49, G143, G234 | TLO (+/−/cancel/5-axis/TCPC) | RAW-семейство G49 |
| 09 | G73, G74, G76, G77, G80, G81–G86, G89, G156 | canned cycles | развёртка §7 / RAW |
| 10 | G98, G99 | возврат цикла | ABSORB (правило §7 SPEC) |
| 11 | G50, G51 | scaling (Haas-нумерация группы!) | RAW(E_UNSUPPORTED_SEMANTICS) |
| 12 | G54–G59, G110–G129, G154 (G52 — «00 или 12» по списку `?`) | WCS | ABSORB (p=1..6) / RAW (§2.3) |
| 14 | G268, G269 | feature CS on/off | RAW (5-осевое) |
| 15 | G61, G64 | exact stop mode / **cancel G61** | META + примечание (§2.2) |
| 16 | G68, G69 | поворот СК | RAW (кандидат этапа 12) |
| 23 | G254, G255 | Dynamic Work Offset (DWO) on/off | RAW (5-осевое) |

Правило «один код на группу на блок» — адресный язык, несколько групп в блоке легальны (`G00 G90 G40 G49 G54`). Групп 04/13/17/18 Fanuc-типа **нет** (arc-center режим, corner override, зеркало-группа: зеркало Haas — G100/G101 в группе 00!).

### 1.2 Формат чисел (критично для квантования §10 SPEC)

| Свойство | Haas (NGC) | Следствие для EID |
|---|---|---|
| Least input increment | **0.001 mm / 0.0001"** (таблицы Setting 77: inch default `.0001`, metric `.001`) | совпадает с Fanuc IS-B; EID-квантование 4 dp покрывает mm (3dp) и inch (4dp) полностью — потерь нет |
| Диапазон | до `?` (мануал недоступен; ожидаемо ±99999.9999-класс) | парсер обязан проверять целую часть → `E_LIMIT_EXCEEDED` |
| **Пропуск decimal point (DPI)** — **Ловушка №1, направление обратное гипотезе** | **Дефолт = machinists notation** (аналог Fanuc DPI=0): без точки значение масштабируется на least increment — inch: `X-2` = −0.0002"; mm: `X-2` = −0.002 mm. Calculator-режим = **Setting 162 «Default To Float» (дефолт OFF)**: добавляет точку к X, Y, Z, A, B, C, E, F, I, J, K, U, W. **Setting 77 «Scale Integer F»** (дефолт OFF): `F12` = 0.0012 units/min; ON: `F12` = 12.0. Исключения: G76 A (угол резца, lathe), D (кроме G73 `?`), R (кроме G71 YASNAC `?`) — токарные нюансы | **Ловушка №1 адаптера.** Bare-integer в словах с дробным доменом (X/Y/Z/A/B/C/E/F/I/J/K [U/W `?`] + **P** — dwell-домен мс) читать по DPI=least-increment **или** `E_INVALID_INPUT`; молча-целое запрещено (тихая ошибка ×100/×1000). Fanuc-политика §9.1(1) переиспользуется |
| Трейлинг-точка | `X25.` легальна (и обязательна в эмиссии) | лексер обязан принимать |
| P word (dwell) | **Единицы зависят от точки** — DPI-правило с гранулярностью мс: `G04 P10.` = 10 **сек**; `G04 P10` (без точки) = 10 **мс** (Helman; официальную страницу G04 сверить — §10) | **отличие от Fanuc (всегда мс) и LinuxCNC (всегда сек)**: развёртка dwell обязана знать формат слова; политика v0.1: точка обязательна, bare → мс-чтение + notice. X/U-формы dwell у Haas **нет** (fallback как у Fanuc-G04 X недоступен) |
| Q word | пек G73/G83 (всегда +); shift-величина G76; feed-override % G12/G13; input# M96; value G167 — **контекстно-зависимое слово** | валидация при развёртке |
| L word | **повтор цикла** (не K!); L0 = запись данных без исполнения `?`; L = повторы M97/M98 | разворачивание §5.2 |
| K word | **НЕ повтор** (как Fanuc!): min-пек G83 / угловой шаг G12 | парсер обязан не переносить Fanuc-K-семантику |

### 1.3 Лексика и структура файла

| Элемент | Haas | Диспозиция |
|---|---|---|
| Конец блока (EOB) | **`;`** — символ EOB (клавиша EOB на пульте вставляет `;`), НЕ комментарий | **Ловушка №2 (та же, что Fanuc).** Лексеры GRBL/LinuxCNC трактуют `;` как комментарий — для Haas-входа `;` = разделитель блоков (эквивалент `\n`) или `E_INVALID_INPUT`; молча выбросить хвост = потеря кодов |
| Комментарии | `( )` — парные, вложенность запрещена; лимит длины ~30–40 символов по поколениям `?` | аннотации (SPEC §11) |
| `%` | маркер начала/конца файла (DNC/transfer) | IGNORED_SAFE |
| N-слова | номера кадров; **цель M97 P-вызова и Macro-B GOTO** | IGNORED_SAFE (в Macro B-файлах строка уже RAW) |
| O-слова | `O#####` — номер программы (NGC — 5 цифр `?`); main + subs в одном файле или отдельных | O-code → RAW(E_UNSUPPORTED_SYNTAX) (v0.16); хвост-subs после program_end — raw-хвост (§4.4) |
| `/` block delete | поддерживается (переключатель станка) | RAW(E_RUNTIME_STATE) (как LinuxCNC/Fanuc) |
| Пустые блоки | легальны | пропуск без диагностики |
| Порядок слов | произвольный (адресный язык) | парсер адресных слов |
| «Голые» координаты без motion-кода | легальны — продолжение группы 01 `?` | ABSORB модальности |
| G92 S / G50 S | **нет** (кламп оборотов Haas — Setting, не G92-форма `?`) | S при G92 → не frame_preset; в словаре Haas-формы нет — `E_INVALID_INPUT`-кандидат, ревью |
| Числовой формат эмиссии | decimal point **обязателен** (DPI-ловушка) | §8 |

---

## 2. COVERAGE-матрица G-кодов (Haas mill NGC)

Колонки: Код | Haas (группа; описание) | Диспозиция | L1 | Примечание / причина.

### 2.1 Движение

| Код | Haas | Диспозиция | L1 | Примечание |
|---|---|---|---|---|
| G00 | 01; rapid | NODE | rapid | 1:1 |
| G01 | 01; linear | NODE | linear | 1:1; F модален. **Haas-специфика: опциональные C/R-слова в G01 = фаска/скругление угла** между этим и следующим ходом — при наличии C/R → RAW(E_UNSUPPORTED_SEMANTICS) (нужен lookahead-декомпозиция; кандидат этапа 12). Контекстное слово: R в G01 — радиус скругления, R в G02/03 — радиус дуги |
| G02/G03 | 01; CW/CCW | NODE | arc | Центр: **I/J/K — инкрементальные от start всегда** (группы 04 нет); R-форма: знак R выбирает ≤180°/>180°; **R при полной окружности невалидна**; полное кольцо = I/J/K без end-слов (официальный ман: «Defining only the I J K values… will cut a circle»); винтовая (Z-слово с G2/G3) — стандарт (thread milling; опция «Helical Motion Enhancement» на CHC `?`, на NGC — в словаре); R+вне-плоскостное слово `?` (§10). Эмиссия — in-plane IJK-пары (COVERAGE §5, механизм GRBL) |
| G12/G13 | 00; Circular Pocket Milling CW/CCW | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Haas-генератор траектории**: I = радиус кармана, J = стартовый угол, K = радиальный шаг за 360°, Q = feed-override % (при K>0), L = повторы (при K=0), Z = глубина. Геометрия статически вычислима из текста → **кандидат развёртки этапа 12** (аналогия §7); v0.1 — RAW |
| G31 | 00; Feed Until Skip | RAW(E_UNSUPPORTED_SEMANTICS) | — | probing-класс G38.x (COVERAGE §2.1); позиция skip-точки — runtime; этап 12 |
| G35/G36/G37 | 00; автоизмерение: диаметр инструмента / work offset / длина инструмента | RAW(E_UNSUPPORTED_SEMANTICS) | — | сенсорные (probe/tool setter), пишут в таблицы станка; класс G31/G37 Fanuc |
| G136 | 00; Automatic Work Offset Center Measurement | RAW(E_UNSUPPORTED_SEMANTICS) | — | probing; референс-точка ставится M75 |

### 2.2 Модальные режимы

| Код | Haas | Диспозиция | L1 | Примечание |
|---|---|---|---|---|
| G17/G18/G19 | 02; плоскости | ABSORB | — | валидация нормали дуги (§6 SPEC); те же пары, что LinuxCNC |
| G20/G21 | 06; inch/mm | META | units | одно переключение на программу (§2.5 SPEC); **псевдонимов G70/G71 НЕТ** — Fanuc-адаптер-аналогия неприменима; коллизия: G70/G71 Haas = bolt-паттерны (§2.6) |
| G90/G91 | 03; abs/inc | ABSORB | — | 1:1 |
| G93/G94 | 05; inverse-time / per-min | G94 ABSORB; G93 ABSORB | state.feed | G93 — 5-осевое/опция; `feed = len·F` §4 SPEC |
| G95 | 05; feed per revolution | **RAW(E_UNSUPPORTED_SEMANTICS)** | state.feed | как Fanuc: фактический rpm зависит от override → статическая эквивалентность не гарантируется; кандидат ABSORB при статическом S (ревизия по корпусу, как COVERAGE §2.2 G95) |
| G60 | 00; Uni-Directional Positioning | RAW(E_UNSUPPORTED_SEMANTICS) | — | overtravel-подход со смещением по одной оси; не выражимо (как Fanuc G60) |
| G09 | 00; Exact Stop (one-shot) | RAW(E_UNSUPPORTED_SEMANTICS) | — | one-shot останов; path_mode L1 — программно-глобальный (§3 SPEC); соседний G61 → META |
| G61 | 15; Exact Stop Mode | META | path_mode = exact_stop | **совпадает по значению с Fanuc G61** (exact stop); LinuxCNC G61 = exact_path — маппинг по значению, не по имени (аналогия Fanuc-строки COVERAGE §2.2) |
| G64 | 15; **G61 Cancel** | META | path_mode = blend (bare) | **сдвиг значения vs LinuxCNC G64 = blend(t): у Haas G64 — только cancel exact-stop, квантификатора P/t нет вовсе.** Это дефолтное поведение Haas; эмиссия blend → G64 + warning (t теряется) — §8 |
| G62/G63 | — | **N/A** | — | corner override / tapping mode в словаре Haas отсутствуют (Haas-аналог G63-поведения живёт внутри G84) |
| G61.1 | — | **N/A** | — | отсутствует |
| G15/G16 | — | **N/A** | — | полярные координаты отсутствуют |
| G96/G97 | — | **N/A (mill)** | — | CSS — токарное; в mill-списке отсутствуют |
| G22/G23, G25/G26, G27, G30 | — | **N/A** | — | отсутствуют в mill-списке (home = G28/G29; 2nd reference нет) |
| G45–G48 | — | **N/A** | — | отсутствуют |
| G12.1/G13.1, G07.1-алиасы | — | **N/A** | — | полярная интерполяция отсутствует; цилиндрическая — **G107** (§2.6) |

### 2.3 Системы координат и фреймы

| Код | Haas | Диспозиция | L1 | Примечание |
|---|---|---|---|---|
| G54–G59 | 12; 6 WCS | ABSORB | state.wcs | 1:1, p=1..6 ⊂ EID p=1..9 |
| **G110–G129** | 12; Coordinate Systems **#7–#26** | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Ловушка №2.** Расширенный WCS: G110=#7 … G129=#26. Вне wcs-enum L1 (p≤9) для G113+; **G110–G112 = слоты #7–#9 = ровно свободные EID-слоты p=7..9** — кандидат ревизии этапа 12 (ABSORB p=7..9; нумерация слотов нативная, см. §8). v0.1 — RAW консервативно |
| **G154 P1–P99** | 12; Select Work Coordinates P1–P99 | RAW(E_UNSUPPORTED_SEMANTICS) | — | 99 доп. СК поверх G54–G59; **G154 P1–P20 = алиасы G110–G129** (Haas UK). Выбор СК вне enum; P7..P26 — те же слоты, что G110–G112+ (§8). Кандидат ревизии этапа 12 |
| G54.1 | — | **N/A** | — | **в словаре Haas отсутствует** (Fanuc-эквивалент — G154 Pn); эмиссия EID→Haas G54.1-запросов невозможна как G54.1 — см. §8 |
| G59.1/.2/.3 | — | **N/A** | — | кодов нет (как у Fanuc); обратная эмиссия — §8 |
| G10 L2 P1–P6 | 00; запись origin G54–G59 | NODE | wcs_define | 1:1; G90/G91 — абсолют/инкремент записи (§5.3 SPEC; Haas-пример `G10 L2 P1 G91 X60.` — официальный) |
| **G10 L2 P0** | 00; запись **слота G52** | RAW(E_REQUIRES_MACHINE_TABLE) | — | **отличие от Fanuc P0:** у Haas L2 P0 = запись work-coordinate **G52** (Haas UK: «P0 G52 references work coordinate (L2)»), а НЕ external/common offset. G52-слот = сдвиг поверх активной WCS — понятие вне L1; связан с ERR-строкой G52 |
| **G10 L20 Pn** | 00; запись origin **G110–G129 / G154 Pn** | RAW(E_REQUIRES_MACHINE_TABLE) | — | **Ловушка №2b: та же, что Fanuc L20.** Haas L20 = auxiliary work coordinate origin write (P1–P20 → G110–G129; P1–P99 → G154 Pn; пример: `G10 L20 P2 G90 X100. Y80.` = «sets offset G111… from the home position» — абсолютная запись в машинных координатах), **НЕ wcs_set_here**. Маппить L20 → wcs_set_here запрещено (правило Fanuc-строки COVERAGE §2.3 применимо дословно) |
| G10 L1/L10/L11/L12/L13 | 00; записи tool offset (L10 = geometry/H; L1/L11 = wear/H; L12 = diameter/D; L13 = diameter wear) | RAW(E_REQUIRES_MACHINE_TABLE) | — | таблицы станка; R-слово = значение; примеры официальные (`G10 L10 G90 P5 R250.`) |
| G11 | — | **N/A** | — | cancel G10 у Haas нет (G10 — one-shot группа 00); **семейство G11 Fanuc-матрицы для Haas не требуется** — G10-формы сами RAW |
| G92 X_ Y_ Z_ | 00; Set Work Coordinate Systems Shift Value | META | frame_preset | 1:1 с GRBL/LinuxCNC/Fanuc; Haas-нюансы: G92/G52 **делят общий регистр сдвига**; интерпретация зависит от **Setting 33 (FANUC/HAAS)** `?`; G92-сдвиг действует от machine zero на все WCS; сброс — reset/power `?` (Setting 129 `?`). Эмиссии hygiene — §8 |
| G92.1 | — | **N/A** | — | **кода сброса сдвига в словаре нет** — reverse-gap: G92.1-hygiene эмиссии невозможна (§8) |
| **G52** | 00 или 12; Set Work Coordinate System (сдвиг поверх активной WCS) | **ERR(E_UNSUPPORTED_SEMANTICS)** | — | **второй жёсткий отказ матрицы (после Fanuc G52, тот же класс SPEC §8 правило 5):** позиционно-зависимая смена фрейма mid-program — raw-сохранение исказило бы последующие ходы. Отмена — `G52 X0 Y0 Z0` (неявная: невыписанные оси сохраняют сдвиг). Haas-усложнения: общий регистр с G92 `?`; **Setting 129** — очистка G52 на M30 `?`; Setting 33 меняет интерпретацию `?`. Кандидат этапа 12 (декомпозиция сдвига; интерференция с WCS-переключениями — как Fanuc) |
| G53 | 00; Non-Modal Machine Coordinate Selection | NODE-поле | state.frame = machine | правило machine-frame §5.3 SPEC (origin обязан быть in-text) |
| G28 | 00; Return To Machine Zero | RAW(E_REQUIRES_MACHINE_TABLE) | — | Q1; с осевыми словами — через промежуточную точку `?` (как Fanuc); позиция machine zero вне текста |
| G29 | 00; Return From Reference Point | RAW(E_REQUIRES_MACHINE_TABLE) | — | позиция reference вне текста; Q1 |
| G30/G27/G30 Pn | — | **N/A** | — | отсутствуют |

### 2.4 Компенсации

| Код | Haas | Диспозиция | L1 | Примечание |
|---|---|---|---|---|
| G40 | 07; Cutter Compensation Cancel | IGNORED_SAFE* | — | правило семейств §1.3 COVERAGE; **Haas-состав семейства G40 = {G41, G42, G141}** (Fanuc-расширений G41.2–.6/G38/G39 нет) |
| G41/G42 (+D) | 07; 2D Cutter Comp Left/Right | RAW(E_UNSUPPORTED_SEMANTICS) | — | comp-геометрия — не-цель §1.2; D = номер радиуса/диаметра в таблице (Setting 40 diameter/radius, Setting 58 Fanuc/Yasnac-style corner behaviour — параметры станка) |
| G141 | 07; 3D+ Cutter Compensation | RAW(E_UNSUPPORTED_SEMANTICS) | — | 3D-comp (X/Y/Z) — 5-осевой класс; этап 12; входит в семейство G40 |
| G43 H_ / G44 H_ | 08; TLO +/− | RAW(E_REQUIRES_MACHINE_TABLE) | — | значение из таблицы (класс COVERAGE §2.4); G43 без H (переактивация) `?`; H-слово = номер в таблице |
| G143 | 08; 5-Axis Tool Length Compensation + | RAW(E_REQUIRES_MACHINE_TABLE) | — | 5-осевое; семейство G49 |
| G49 | 08; G43/G44/G143 Cancel | IGNORED_SAFE* | — | **Haas-состав семейства G49 = {G43, G44, G143}** (G43.1/.2/.4/.5 в словаре нет — N/A); cancel по H00 `?` |
| G43.1/G43.2/G43.4/G43.5 | — | **N/A** | — | dynamic TLO/TCP-форм отсутствуют (TCP-класс живёт в G234) |
| G234 | 08; Tool Center Point Control (TCPC) | RAW(E_UNSUPPORTED_SEMANTICS) | — | 5-осевое (RTCP-класс), этап 12 |

### 2.5 Canned cycles (детали — §5)

| Код | Haas | Диспозиция | L1 | Примечание |
|---|---|---|---|---|
| G80 | 09; cancel cycle | IGNORED_SAFE | — | как COVERAGE §2.1 |
| G81 | 09; Drill | разворачивается | rapid/linear | формат §5.1 |
| G82 | 09; Spot Drill (+dwell) | разворачивается | rapid/linear/dwell | P — dwell (единицы по точке: `?` §10) |
| G83 | 09; Normal Peck Drilling | разворачивается | rapid/linear/dwell | **Haas-семантика отличается от Fanuc/LinuxCNC:** пеки — **Q (константный) ИЛИ I/J/K (переменный: I = первый пек, J = уменьшение шага, K = минимальный пек; официальная формулировка lathe-страницы; на mill `?`)**; **Q и I/J/K mutually exclusive** («Do not use a Q value when programming with I, J, and K»); полный отвод к R каждый пек; **Setting 52 «G83 Retract Above R»** ≠ 0 → отвод выше R `?`; dwell на дне P `?`; развёртка §5.4 |
| G73 | 09; High-Speed Peck (chip-break) | разворачивается | rapid/linear | **отвод между пеками = Setting 22 «Canned Cycle Delta Z»** (не Q, как Fanuc; не фикс. 0.5 мм, как LinuxCNC!) — **документированный дефолт Setting 22 = 0.02"/0.5 mm совпадает с канон-константой SPEC §7.4a**; переменные пеки I/J/K на G73 `?`; δ → константа 0.5 мм + notice `W_CYCLE_PARAM_ASSUMED` (setting может быть изменён оператором — вне текста) |
| G74 | 09; Reverse Tap | RAW(E_UNSUPPORTED_SEMANTICS) | — | синхронизация/реверс — класс G84 |
| G84 | 09; Tapping | RAW(E_UNSUPPORTED_SEMANTICS) | — | rigid — внутренне (Setting 57 Rigid Tap / опция; NGC — rigid-режим), **M29 НЕ нужен и НЕ существует как rigid-tap** (коллизия с Fanuc M29!); Q = peck tap (NGC); rigid/peck-модификации не меняют класс RAW |
| G76 | 09; Fine Boring | RAW(E_UNSUPPORTED_SEMANTICS) | — | **двойной отказ с отличием от Fanuc:** (а) ориентация шпинделя на дне (M19-класс), (б) сдвиг Q. **Направление сдвига — I/J-слова в тексте** (X/Y-компоненты), а НЕ параметр станка как у Fanuc (5101) → при будущем решении M19-класса кандидат этапа 12 (все параметры в тексте) |
| G77 | 09; Back Bore | RAW(E_UNSUPPORTED_SEMANTICS) | — | **Haas-замена Fanuc G87** (back boring); ориентация+сдвиг+обход стенки — не представимо ходами; **G87/G88 в словаре Haas отсутствуют** (N/A) |
| G85 | 09; Boring (feed out) | разворачивается | rapid/linear | 1:1 с LinuxCNC |
| G86 | 09; Bore and Stop | разворачивается | rapid/linear/dwell + state(spindle) | **официально: feed → dwell P `?` → spindle stop → rapid out → spindle restart** — рестарт подтверждён документацией (как LinuxCNC Q9); развёртка §7 SPEC применима |
| G89 | 09; Bore In, Dwell, Bore Out | разворачивается | rapid/linear/dwell | feed-out (как LinuxCNC §7.4 SPEC v0.14) |
| G156 | 09; Broaching | RAW(E_UNSUPPORTED_SEMANTICS) | — | broaching-операция (механика инструмента); аргументы `?` |
| G174 | 00; CCW Non-Vertical Rigid Tap | RAW(E_UNSUPPORTED_SEMANTICS) | — | 4/5-осевое нарезание резьбы; класс G84 |
| G184 | 00; CW Non-Vertical Rigid Tap | RAW(E_UNSUPPORTED_SEMANTICS) | — | см. G174 |
| G98/G99 | 10; initial point / R plane return | ABSORB | — | правило §7 SPEC; **lathe-коллизия как у Fanuc:** на Haas-lathe G98/G99 = feed-режимы — смешение lathe/mill-файлов → `E_INVALID_INPUT`; профиль различает M/T-контекст. Дефолт без G98/G99 — `?` (§10; SPEC §7.5a provisional G99) |
| K0/L0-спецсемантика | — | **N/A** | — | Fanuc-K0 («движение по одной оси») в словаре Haas нет; **L0** в цикле = запись данных без исполнения `?` (§10) — класс: при встрече `?` → E_UNSUPPORTED_SEMANTICS-кандидат до сверки |

### 2.6 Преобразования геометрии и Haas-генераторы траекторий

Все — параметры в тексте → геометрия в принципе статически вычислима; v0.1 = RAW; развёртка — отдельное решение этапа 12 (аналогия G5 scope decision / Fanuc G68).

| Код | Haas | Диспозиция | L1 | Примечание |
|---|---|---|---|---|
| G68/G69 | 16; Rotation / cancel (опция rotation & scaling) | RAW(E_UNSUPPORTED_SEMANTICS) | — | кандидат статической декомпозиции; интерференция с WCS/G52-зонами — как Fanuc |
| G50/G51 | 11; Cancel Scaling / Scaling (опция) | RAW(E_UNSUPPORTED_SEMANTICS) | — | **группа 11** (не 18/00, как в Fanuc-спорах) — по официальному списку; масштаб/фактор в тексте |
| G100/G101 | 00; Cancel / Enable Mirror Image | RAW(E_UNSUPPORTED_SEMANTICS) | — | **зеркало — G100/G101** (НЕ G50.1/G51.1, которых нет); оси зеркала — Settings 45–48/80/250 (вне текста!), pivot — X-слово; зеркало меняет climb↔conventional (аргумент против «free»-декомпозиции). Историческая двойная семантика G100/G101 (rotary lock/unlock с P) на NGC `?` |
| G107 | 00; Cylindrical Mapping | RAW(E_UNSUPPORTED_SEMANTICS) | — | нелинейный маппинг осей (класс Fanuc G07.1); опция |
| G47 | 00; Text Engraving | RAW(E_UNSUPPORTED_SEMANTICS) | — | генерация гравировки из текстовых аргументов (глифы — внутренние таблицы контроля) → machine-table-класс по глифам; кандидат этапа 12 при экспорте глифов |
| G150 | 00; General Purpose Pocket Milling | RAW(E_UNSUPPORTED_SYNTAX) | — | геометрия кармана — **подпрограмма** (P = O-номер) → subs-механика, вне L1 вдвойне |
| G70/G71/G72 | 00; Bolt Hole Circle / Arc / Holes Along an Angle | RAW(E_UNSUPPORTED_SEMANTICS) | — | **коллизия адресов:** Fanuc mill G70/G71 = units-псевдонимы, Haas = болтовые паттерны (аргументы-геометрия в тексте). Генерируют позиции для активного cycle-кода → кандидат развёртки этапа 12 (позиции вычислимы); v0.1 — RAW |

### 2.7 Прочее

| Код | Haas | Диспозиция | L1 | Примечание |
|---|---|---|---|---|
| G04 P_ | 00; Dwell | NODE | dwell | **P — сек при точке / мс без точки** (§1.2); X/U-форм нет; `G04 P0` — drop+notice (§3/§5.2 SPEC) |
| G09 | 00; Exact Stop | RAW(E_UNSUPPORTED_SEMANTICS) | — | см. §2.2 |
| G103 P0–15 | 00; Limit Block Buffering | RAW(E_UNSUPPORTED_SEMANTICS) | — | лимит look-ahead (P0 = отмена); геометрию не меняет, но влияет на corner rounding/M1-исполнение; пары с probing/macro; класс G09/G60 (execution control) |
| G167 | 00; Modify Setting | RAW(E_RUNTIME_STATE) | — | мутация машинной настройки mid-program (setting# + значение в тексте `?` формат); исполнение последующих опов зависит от setting — runtime-класс (аналогия block delete); потенциальный Err-класс (аналог смены единиц) — флаг на ревью |
| G187 | 00; Setting the Smoothness Level | RAW(E_UNSUPPORTED_SEMANTICS) | — | accuracy-контроль HSM: E = допустимое отклонение (`G187 E0.005`); CHC-форма P1–P4 `?`; влияет на rounding траектории (допуск) — семантический сосед LinuxCNC G64 P; one-shot/модальность `?` → v0.1 RAW, кандидат META-ревизии |
| G266 | 00; Visible Axes Linear Rapid % Motion | RAW(E_UNSUPPORTED_SEMANTICS) | — | rapid с процентным ограничением скорости — скорость rapid в L1 не представима; геометрия = linear (кандидат ABSORB при игнорировании % — против: меняет фактическое движение); ревью |
| G253 | 00; Orient Spindle Normal To Feature CS | RAW(E_UNSUPPORTED_SEMANTICS) | — | 5-осевое (M19-класс + feature CS) |
| G254/G255 | 23; Dynamic Work Offset enable/cancel | RAW(E_UNSUPPORTED_SEMANTICS) | — | DWO — 5-осевое (поворот СК вокруг оси rotary) |
| G268/G269 | 14; Feature Coordinate System enable/cancel | RAW(E_UNSUPPORTED_SEMANTICS) | — | 5-осевое |
| G05/G05.1/G08, G33, G06.2, G02.2/.3/.4 и пр. | — | **N/A** | — | вендорские Fanuc-коды в словаре Haas отсутствуют (парсер Haas их не встретит валидно; если встретит — generic unknown-G ведро: RAW(E_UNSUPPORTED_SYNTAX)) |
| Неизвестный G-код (вне словаря профиля) | — | RAW(E_UNSUPPORTED_SYNTAX) | — | generic-ведро + диагностика текста — не молча (правило Fanuc-матрицы) |

---

## 3. COVERAGE-матрица M-кодов (Haas mill NGC)

Источник — официальный список «Mill — M-Codes». Фиксированная семантика: M00/M01/M02/M30/M03–M06/M16/M08/M09; остальные — вендорские, но **документированные Haas** (не произвольные builder-specific): для документированного machine-I/O используется класс SEMANTICS (по аналогии с LinuxCNC-строками M62–M68); user-definable и control-flow — SYNTAX. Неизвестный M (вне словаря профиля) → RAW(E_UNSUPPORTED_SYNTAX) + диагностика — не молча.

| Код | Haas | Диспозиция | L1 | Примечание |
|---|---|---|---|---|
| M00 | Stop Program | NODE | program_stop(optional=no) | 1:1 |
| M01 | Optional Program Stop | NODE | program_stop(optional=yes) | Q8 (настройка станка вне L1) |
| M02 | Program End | NODE | program_end | различие M02/M30 вне L1 |
| M30 | Program End and Reset | NODE | program_end | дефолт эмиссии — §8 |
| M03/M04/M05 | шпиндель CW/CCW/stop | ABSORB | state.spindle | 1:1; S-слово без M03 — сохранение оборотов |
| M06 / M16 | Tool Change (M16 — альтернативный код смены) | NODE | tool_change | идиома `T01 M06` (как Fanuc); **T-слово отдельно = предвыбор next-tool** (side-mount SMTC) → RAW(E_UNSUPPORTED_SEMANTICS), кандидат `selected_tool` (как Fanuc/LinuxCNC); M6 не применяет TLO (применяет G43) `?` restart |
| M07 | Shower Coolant On | RAW(E_UNSUPPORTED_SEMANTICS) | — | **второй канал СОЖ** (не mist!); state.coolant L1 = {mist, flood} — третий канал не представим; кандидат ревизии coolant-модели |
| M08 / M09 | Coolant On / Off | ABSORB | state.coolant.flood / none | 1:1 |
| M10 / M11 | Engage / Release **4th Axis Brake** | RAW(E_UNSUPPORTED_SEMANTICS) | — | 4/5-осевая механика, этап 12; ABSORB «врал» бы |
| M12 / M13 | Engage / Release **5th Axis Brake** | RAW(E_UNSUPPORTED_SEMANTICS) | — | **гипотеза «M13/M14 = шпиндель+СОЖ» опровергнута** — это generic Fanuc-идиома; в Haas mill M13 = brake 5-й оси (коллизия номеров между диалектами — аргумент за cross-dialect отказами) |
| M19 | Orient Spindle | RAW(E_UNSUPPORTED_SEMANTICS) | — | ориентация (угол — R-слово `?`); энкодер-класс |
| M21–M25 | Optional User M Function (с M-Fin) | RAW(E_UNSUPPORTED_SYNTAX) | — | user-definable — вне словаря профиля |
| M29 | Set Output Relay (с M-Fin) | RAW(E_UNSUPPORTED_SEMANTICS) | — | **коллизия с Fanuc M29 (rigid tap): на Haas — relay-выход** (класс LinuxCNC M62); rigid tap Haas = Setting 57 + G84, без M29 |
| M31 / M33 | Chip Conveyor Forward / Stop | RAW(E_UNSUPPORTED_SEMANTICS) | — | периферия (I/O) |
| M34 / M35 | Coolant Increment / Decrement | RAW(E_UNSUPPORTED_SEMANTICS) | — | TSC-давление/концентрация — вне coolant-модели L1 |
| M36 | Pallet Part Ready | RAW(E_UNSUPPORTED_SEMANTICS) | — | паллеты |
| M39 | Rotate Tool Turret | RAW(E_UNSUPPORTED_SEMANTICS) | — | periphery |
| M41 / M42 | Low / High Gear Override | RAW(E_UNSUPPORTED_SEMANTICS) | — | override = process intent (класс LinuxCNC M48/M49) |
| M46 | Jump to Line (Qn P… — формулировка списка `?`) | RAW(E_UNSUPPORTED_SYNTAX) | — | control-flow-семейство (как M96/M99); точная семантика — §10 |
| M48 | Validate Program For Loaded Pallet | RAW(E_UNSUPPORTED_SEMANTICS) | — | паллетная автоматика |
| M50 | Pallet Change Sequence | RAW(E_UNSUPPORTED_SEMANTICS) | — | паллеты (класс LinuxCNC M60) |
| M51–M55 | Set Optional User M-codes | RAW(E_UNSUPPORTED_SYNTAX) | — | user-definable |
| M59 / M69 | Set / Clear Output Relay | RAW(E_UNSUPPORTED_SEMANTICS) | — | цифровые выходы (класс M62/M68) |
| M61–M65 | Clear Optional User M-codes | RAW(E_UNSUPPORTED_SYNTAX) | — | user-definable; **коллизия номеров с LinuxCNC M61 (аналог state.tool) и M62–M65 (outputs)** — профили различают |
| M70 / M71 | Workholding Clamp / Unclamp | RAW(E_UNSUPPORTED_SEMANTICS) | — | **коллизия с LinuxCNC M70–M73 (стек модального состояния)** — семантика другая |
| M73 / M74 | Tool Air Blast (TAB) On / Off | RAW(E_UNSUPPORTED_SEMANTICS) | — | периферия |
| M75 | Set G35 or G136 Reference Point | RAW(E_UNSUPPORTED_SEMANTICS) | — | probing-сервис |
| M78 / M79 | Alarm if Skip Signal Found / Not Found | RAW(E_UNSUPPORTED_SEMANTICS) | — | probing-диагностика |
| M80 / M81 | Auto Door Open / Close | RAW(E_UNSUPPORTED_SEMANTICS) | — | периферия |
| M82 / M86 | Tool Unclamp / Tool Clamp | RAW(E_UNSUPPORTED_SEMANTICS) | — | периферия |
| M83 / M84 | Auto Air Gun On / Off | RAW(E_UNSUPPORTED_SEMANTICS) | — | периферия |
| M88 / M89 | Through-Spindle Coolant On / Off | RAW(E_UNSUPPORTED_SEMANTICS) | — | TSC — отдельный канал СОЖ (вне модели L1); кандидат ревизии coolant-модели |
| M90 / M91 | Fixture Clamp Input On / Off | RAW(E_UNSUPPORTED_SEMANTICS) | — | периферия |
| M95 | Sleep Mode | RAW(E_UNSUPPORTED_SEMANTICS) | — | энергосбережение с таймером (не dwell-оп) |
| M96 | Jump If No Input (P = line, Q = input) | RAW(E_UNSUPPORTED_SYNTAX) | — | условный control-flow по входу; по смыслу вход-ожидание — E_RUNTIME_STATE (аналог M66), но контроль потока → SYNTAX-семейство; ревью |
| M97 | Local Sub-Program Call (`M97 P#### L_` — P = **N-номер** в том же файле) | RAW(E_UNSUPPORTED_SYNTAX) | — | Haas-специфика (нет у Fanuc); часть subs-механики §4.2 |
| M98 | Sub-Program Call (`M98 P<O#> L_`) | RAW(E_UNSUPPORTED_SYNTAX) | — | как Fanuc; **слип-формата P<L><####> у Haas нет** (повтор — только L) |
| M99 | Sub-Program Return or Loop (`M99 P####` = переход на N####) | RAW(E_UNSUPPORTED_SYNTAX) | — | M99 в main = бесконечный цикл (Setting 118 `?`) — структурно не статичен |
| M104 / M105 | Probe Arm Extend / Retract | RAW(E_UNSUPPORTED_SEMANTICS) | — | периферия |
| M109 | Interactive User Input (P = формат, запись в `#`-переменную `?`) | RAW(E_RUNTIME_STATE) | — | ожидание ввода оператора = состояние исполнения (аналог LinuxCNC M66); макро-семейство |
| M116 / M117 | Vise Air Chips Blast On/Off | RAW(E_UNSUPPORTED_SEMANTICS) | — | периферия |
| M130 / M131 | Display Media / Cancel | RAW(E_UNSUPPORTED_SYNTAX) | — | операторский UI (картинка/видео на экране) — не движение; кандидат IGNORED_SAFE при ревью (аналогия N/`%` — process-нейтральность), v0.1 — RAW не молча |
| M138 / M139 | Spindle Speed Variation On / Off | RAW(E_UNSUPPORTED_SEMANTICS) | — | override-класс (process intent) |
| M158 / M159 | Mist Condenser On / Off | RAW(E_UNSUPPORTED_SEMANTICS) | — | mist — канал L1 есть, но Haas-код ≠ M07 Fanuc- mist-семантика; коллизия номеров с Fanuc M07 (mist) — профиль различает; ABSORB-кандидат при ревью coolant-модели |
| M160–M163 | PulseJet (cancel/continuous/single/modal) | RAW(E_UNSUPPORTED_SEMANTICS) | — | периферия СОЖ |
| M180 / M181 | Auto Window Open / Close | RAW(E_UNSUPPORTED_SEMANTICS) | — | периферия |
| M199 | Pallet / Part Load or Program End | RAW(E_UNSUPPORTED_SYNTAX) | — | без APL действует как program end `?` — до сверки не эмитить как PEND |
| M300 | APL/Robot Custom Sequence | RAW(E_UNSUPPORTED_SYNTAX) | — | робототехника |
| M вне словаря профиля | — | RAW(E_UNSUPPORTED_SYNTAX) | — | generic-ведро + диагностика |
| T-слово | номер инструмента | ABSORB только в паре T..M06/M16; одиночное T → RAW(E_UNSUPPORTED_SEMANTICS) (предвыбор) | state.tool | кандидат `selected_tool` (ревью §7.5 COVERAGE) |

---

## 4. Прочий словарь: слова, переменные, Macro B, подпрограммы

### 4.1 Слова адресов

| Слово | Haas семантика | Диспозиция |
|---|---|---|
| X/Y/Z | координаты (+ pivot в G101, цели в probing) | ABSORB (опы) |
| I/J/K | **контекстно-зависимые:** центр дуги (инкремент от start) / пеки G83 (I первый, J уменьшение, K минимум) / радиус-угол G12/G13, G70–G72 / направление сдвига G76 | ABSORB (дуга §6) / ABSORB (развёртка §5) / RAW (генераторы §2.6) — парсер обязан разрешать контекст по G-коду блока |
| R | радиус дуги / R-plane циклов / **радиус скругления угла в G01** / угол M19 `?` | ABSORB / RAW (в составе G01 C/R-строки) |
| C | **фаска угла в G01** (Haas-специфика); ротационная ось C | RAW(E_UNSUPPORTED_SEMANTICS) в G01-контексте; ось C — RAW (этап 12) |
| F | подача; **Setting 77** масштабирует bare-integer F | ABSORB (state.feed); DPI-правило §1.2 |
| S | обороты; кламп-форм (G92 S/G50 S) нет | ABSORB (spindle) |
| T | инструмент / предвыбор next-tool | ABSORB только в паре с M06/M16; одиночное T → RAW(E_UNSUPPORTED_SEMANTICS) |
| H / D | номера смещений длины/радиуса в таблице | RAW(E_REQUIRES_MACHINE_TABLE) (в составе G43/G44/G41/G42) |
| P | dwell (мс/сек по точке) / G103 P0–15 / M97 P=N# / M98 P=O# / G47 / M109 / G150 | контекстно; ABSORB (dwell) / RAW (прочее) |
| Q | пек G73/G83 / shift G76 / feed-% G12/G13 / input M96 / value G167 | контекстно; ABSORB (пек) / RAW (прочее) |
| L | **повтор цикла/M97/M98** (K — не повтор!) | ABSORB (развёртка §5.2) |
| E | accuracy G187; входит в Setting 162-домен | RAW (в составе G187-строки) |
| A/B/C | ротационные оси | RAW(E_UNSUPPORTED_SEMANTICS) ×все (этап 12); исключение: A-слово в G12/G13/G70-контексте — часть RAW-строки генератора |
| U/W | Setting 162 упоминает U/W (lathe-наследие `?`); на mill вне словаря | RAW(E_UNSUPPORTED_SEMANTICS) `?` |
| D (в G73 `?`) / R (в G71 YASNAC `?`) | Setting 162-исключения (lathe-нюансы) | `?` — реестр |

### 4.2 Подпрограммы (M97/M98/M99)

- `M98 P<O#> Lnn` — вызов программы O#; повтор L (дефолт 1). Слип-формата Fanuc `P<L><####>` нет.
- `M97 P<N#> Lnn` — вызов **локальной** подпрограммы по N-номеру в том же файле (Haas-специфика; требует N-слов).
- `M99` — возврат; `M99 P####` — возврат с переходом на N####; **M99 в main = loop** (Setting 118 `?` — число циклов/условие).
- Вложенность `?` (Fanuc — 4 уровня; Haas NGC — сверить, реестр).
- M198 (внешний вызов карты/DNC) — **в списке Haas отсутствует** (N/A; DNC-вызов — механика transfer).
- **Диспозиция:** всё семейство — `RAW(E_UNSUPPORTED_SYNTAX)` line-level (v0.16); Q10-инлайнинг без параметров — кандидат ревизии (как Fanuc).

### 4.3 Macro B (`#`-переменные, G65/G66/G67)

**Детект:** `#`-слова; `[ ]`-выражения; IF/GOTO/THEN; WHILE/DO/END; арифметика `#100 = #100 + 1`; VPS-шаблоны (генерируют макро-программы — попадают в тот же bucket).

- **Совместимость с Fanuc Macro B:** G65 (группа 00) / **G66/G67** (модальный вызов/отмена — официальная страница NGC существует, в quick-списке отсутствует `?`); аргументы A–Z → локальные `#1–#33` (маппинг Fanuc-совместимый `?` сверить таблицу); IF/GOTO/WHILE DO/END; операции EQ/NE/GT/LT/GE/LE, AND/OR/XOR, SQRT/LN/EXP/ROUND/FUP/FIX/ABS/MOD/SIN/COS/TAN/ASIN/ACOS/ATAN; углы — градусы `?`.
- **Переменные (отличия от Fanuc):**

| Диапазон | Haas | Fanuc-аналог |
|---|---|---|
| #1–#33 | локальные (args G65) | то же |
| #100–#199 | глобальные volatile (сброс при off) | #100–#199 |
| **#500–#699** | глобальные non-volatile («Settings»-класс) | Fanuc #500–#999 (диапазоны различаются!) |
| **#800–#999** | глобальные (часть зарезервирована Haas: probes, pallets `?`) | — |
| #1000+ | system variables (позиции, модальное состояние, таймеры, alarms) | есть, **нумерация отличается от Fanuc** — кросс-диалектный перенос macro-файлов невозможен без таблицы соответствий |
| **#10000s / #109xxx** | NGC-расширения (retained, зеркала настроек, axis loads, machine status) | Fanuc-расширения другие |

- **Диспозиция (по SPEC §1.1 v0.16):** всё семейство — line-level `RAW(E_UNSUPPORTED_SYNTAX)`; чтение system vars по смыслу `E_RUNTIME_STATE`, но в матрице v0.1 не различается (принцип: не декомпозиция, Raw); правило двух классов — как Fanuc (§4.3 fanuc-отчёта): соседние статические ходы парсятся, точки разрыва → `unknown`; кросс-диалект — `E_RAW_UNTRANSLATABLE`; same-dialect — R1-text. Условная смена фрейма/единиц в Macro B не детектируется — известное ограничение (как Fanuc).

### 4.4 Структура файла

- Первый блок — `O#####` (номер программы) + комментарии; `%` — маркеры transfer/DNC.
- Main + subs в одном файле: после первого program_end следуют `O`-тела subs → **весь хвост после program_end = raw-блок** (E_UNSUPPORTED_SYNTAX; program_end обязан быть последним опом — §3 SPEC; same-dialect R1-text сохраняет; кросс-диалект — отказ целиком).
- Файл без M30/M02 (DNC-выгрузка) — auto-PEND (v0.16, `W_MISSING_PROGRAM_END`) применим; хвост-subs-детект `?` (как Fanuc §4.4).
- Modal power-on defaults Haas (G00/G17/G40/G49/G54/G80/G90/G94 + units по Setting `?`) — на эквивалентность не влияет (assumption самодостаточности, EQUIVALENCE §1.2).

---

## 5. Canned cycles: Haas ↔ Fanuc/LinuxCNC — таблица параметров и расхождений

### 5.1 Общий формат

```
G98|G99 G8x X_ Y_ Z_ R_ [Q_ | I_ J_ K_] [P_] F_ L_
повторы: последующие блоки X/Y без G8x — цикл модален (группа 09); L — число повторов
```

| Параметр | Haas | Fanuc | LinuxCNC | Совпадение |
|---|---|---|---|---|
| X/Y | позиция отверстия | то же | то же | да |
| Z | дно (abs/inc) | то же | то же | да |
| R | R-plane | то же | то же | да |
| Q | пек (G73/G83), всегда +; shift G76; **в G84 = peck tap (NGC)** | пек/shift | пек | да (с оговорками) |
| I/J/K | **G83: I = первый пек, J = уменьшение шага, K = мин. пек** (переменный пекинг; mutually exclusive с Q); G12/G13: радиус/угол | I/J/K центр дуги (K в цикле = повтор 0–9999 `?`-L) | K = повтор цикла | **НЕТ — K-семантика конфликтуает с Fanuc/LinuxCNC** |
| P | dwell: **с точкой = сек, без точки = мс** (`?` официальная сверка) | целые мс | сек float | **НЕТ — DPI-зависимые единицы** |
| F | подача | то же | то же | да |
| **L/K** | **L = повторы** (L0 = запись без исполнения `?`) | K = повторы (0–9999), L на части серий | L | **НЕТ — имя слова инвертировано vs Fanuc** |
| G98/G99 | группа 10 | группа 10 (mill) | то же | да |
| Дефолт без G98/G99 | `?` | `?` (G99-практика) | source: G99 | согласовано с SPEC §7.5a (provisional G99) |

### 5.2 Повторы L: правила

- G90: каждый следующий блок с новыми X/Y = новое отверстие; L в блоке = повторы на той же позиции.
- G91: X/Y-слова — шаг между отверстиями; L = число повторов (Haas workbook: «G81 … Lnn … L5 repeats 5 times»).
- L0 — запись/изменение параметров цикла без исполнения `?` (§10); встречено → не разворачивать, raw-диагностика до сверки.

### 5.3 Расхождения семантики per-cycle

| Цикл | Haas | Fanuc | LinuxCNC | Вывод для канонизатора |
|---|---|---|---|---|
| G73 | отвод между пеками = **Setting 22** (дефолт 0.02"/0.5 mm); пеки Q (или I/J/K `?`) | отвод = Q; δ = параметр 5100-серии `?` | отвод фикс. 0.5 мм | разворачивать по семантике источника; δ_Haas вне текста → константа 0.5 мм + notice `W_CYCLE_PARAM_ASSUMED` (**дефолт Setting 22 = канон-константе — расхождение только при изменённой настройке**) |
| G83 | полный отвод к R (Setting 52 ≠ 0 → выше R `?`); пеки Q **или переменные I/J/K** | полный отвод; возврат до `d_k − δ` (параметр) | полный отвод; δ = 0.5 мм фикс. | переменный пекинг I/J/K — **Haas-расширение**: разворачивать по формуле d_1 = I, d_k = max(K, d_{k−1} − J); Setting 52 — вне текста → notice |
| G82/G86/G89 | P dwell (DPI-единицы `?`) | P мс | P сек | конверсия по правилу §1.2 (точка обязательна в эмиссии) |
| G86 | dwell P `?` → spindle stop → rapid out → **restart** (официально) | stop → rapid out → restart | restart documented | развёртка §7 SPEC (Q9) применима к Haas |
| G76 | orient на дне + сдвиг Q, **направление I/J в тексте** | orient + Q, направление = параметр 5101 | (G76 = токарный) | RAW v0.1; при решении M19-класса — кандидат этапа 12 (все параметры в тексте, в отличие от Fanuc) |
| G77 | back bore (вход в отверстие со сдвигом, bore вверх) | (нет; аналог G87) | «accepted, undefined» (G87) | RAW; Haas-специфичный код |
| G84/G74 | tap; rigid внутренне (Setting 57); **M29 не используется** | tap; rigid = G84.2/.3+M29 | tap | RAW (класс §2.5); Haas-файлы не содержат M29-rigid — парсер не должен требовать |
| G156 | broaching | нет | нет | RAW |

### 5.4 Правило развёртывания Haas (частная кейса общего §7 SPEC)

Единственные отличия от шаблона §7.7 SPEC: (а) **δ G73 = Setting 22** → константа 0.5 мм + notice (дефолт Setting 22 совпадает с константой — golden'ы зеркалят); (б) **переменный пекинг G83 I/J/K** — разворачивать пики по формуле §5.3 (порядок пиков сохранён — §7.4 SPEC); (в) **dwell P — DPI-единицы**: bare-integer → мс-конверсия, с точкой → сек (политика v0.1: точка обязательна при развёртке; несоответствие → notice/E_INVALID_INPUT по решению ревью); (г) L-повторы разворачиваются как K в §7.2 SPEC. Эквивалентность: AST GRBL↔Haas-развёрток расходятся в «подход»-ходах по построению — R3 Tier A не затронут (аналогия Fanuc §5.4).

---

## 6. Маппинг 1:1 — что в Haas ЕСТЬ и в EID/L1 ЕСТЬ

| Haas | → L1 | Комментарий |
|---|---|---|
| G00 | rapid | полное совпадение |
| G01 (без C/R) | linear | полное; C/R-углы → RAW |
| G02/G03 (IJK inc; R ±; helical; full-circle) | arc (§6 SPEC) | центр — GRBL-семантика (всегда инкрементальный, группы 04 нет); R-конвертация; полное кольцо — IJK без end-слов |
| G17/G18/G19 | ABSORB | полное |
| G20/G21 | META(units) | полное; псевдонимов нет |
| G90/G91 | ABSORB | полное |
| G93/G94 | ABSORB (state.feed) | G93 — опция 5-осевая |
| G04 P | dwell (сек, при точке) | DPI-правило для bare-P |
| G54–G59 | state.wcs p=1..6 | подмножество p=1..9 |
| G10 L2 P1–P6 | wcs_define | 1:1 |
| G92 X Y Z | frame_preset | 1:1 (Setting 33/129-нюансы `?`) |
| G53 | state.frame = machine | 1:1 + правило §5.3 |
| G80 | IGNORED_SAFE | полное |
| G98/G99 | ABSORB (правило циклов) | полное |
| G81/G82/G83/G73/G85/G86/G89 | развёртка §7 | по Haas-семантике (§5): L-повторы, P-DPI, δ=Setting 22 |
| M00/M01 | program_stop | полное |
| M02/M30 | program_end | полное (различие вне L1) |
| M03/M04/M05 + S | state.spindle | полное |
| M06/M16 (+T) | tool_change | полное; T-предвыбор — отдельная строка |
| M08/M09 | state.coolant.flood | полное (M07/M88/M89 — другие каналы) |
| N, `%`, `( )` | IGNORED_SAFE / аннотации | `;` — ловушка EOB (§1.3) |
| G61 | META exact_stop | совпадает по значению с Fanuc-G61 |
| G64 | META blend (bare) | **сдвиг значения vs LinuxCNC** (G64 = cancel exact-stop) |

Итог: статическое CAM-подмножество Haas (типовой NGC-пост: G0/G1/G2/G3, G54, G43/G41, G81-семейство, M-стандарт) — **выразимо в L1 полностью**; строки с таблицами (G43 H, G41 D) → RAW(E_REQUIRES_MACHINE_TABLE) с продолжением парса — same-dialect round-trip валиден.

---

## 7. Gap-анализ: Haas ЕСТЬ / EID/L1 НЕТ

### 7.1 RAW-класс (этап 10+, значение вне текста или runtime)

| Haas | Код отказа | Почему не представимо |
|---|---|---|
| G43/G44 H_, G143 | E_REQUIRES_MACHINE_TABLE | TLO из таблицы |
| G41/G42 (+D), G141 | E_UNSUPPORTED_SEMANTICS | comp-геометрия — не-цель §1.2 |
| G28/G29 | E_REQUIRES_MACHINE_TABLE | machine zero из параметров (Q1) |
| G10 L1/L10/L11/L12/L13 | E_REQUIRES_MACHINE_TABLE | таблицы инструментов |
| G10 L20 (aux WCS write) | E_REQUIRES_MACHINE_TABLE | запись origin доп. СК — НЕ wcs_set_here (Ловушка №2b) |
| G10 L2 P0 (G52-slot write) | E_REQUIRES_MACHINE_TABLE | G52-слот вне модели L1 (связан с ERR-строкой G52) |
| G31/G35/G36/G37/G136, M75/M78/M79 | E_UNSUPPORTED_SEMANTICS | сенсорные/пробинг (этап 12) |
| G84/G74/G174/G184 | E_UNSUPPORTED_SEMANTICS | синхронизация со шпинделем |
| G76 | E_UNSUPPORTED_SEMANTICS | ориентация M19-класса + сдвиг (направление — в тексте; кандидат этапа 12) |
| G77/G156 | E_UNSUPPORTED_SEMANTICS | back-bore/broaching механика |
| G62/G63/G09/G60-класс | E_UNSUPPORTED_SEMANTICS | one-shot path-поведение (G09/G60; G62/G63 — N/A) |
| G95 | E_UNSUPPORTED_SEMANTICS | feed/rev — rpm от override (кандидат при статическом S) |
| G68/G69, G50/G51, G100/G101, G107, G47 | E_UNSUPPORTED_SEMANTICS | трансформации/генераторы — кандидаты этапа 12; зеркало меняет climb/conventional; G47 — глифы контроля |
| G12/G13, G70/G71/G72, G150*, G266, G103, G187 | E_UNSUPPORTED_SEMANTICS / *SYNTAX (G150 — subs) | генераторы траекторий/execution-контроль — кандидаты этапа 12 |
| G167 | E_RUNTIME_STATE | мутация настроек mid-program (Err-флаг ревью) |
| M19, M10–M13 (brakes), M29 (relay), M41/M42, M138/M139, M95, I/O-ведро (M31–M91, M104–M181) | E_UNSUPPORTED_SEMANTICS | ориентация/механика/override/периферия |
| M109 | E_RUNTIME_STATE | ввод оператора |
| `/` block delete | E_RUNTIME_STATE | зависит от переключателя |
| G52 | **ERR(E_UNSUPPORTED_SEMANTICS)** | позиционная смена фрейма mid-program (§2.3) |

### 7.2 RAW-класс (Macro B / параметрика — вне статического подмножества)

`#`-слова (все диапазоны §4.3), `[ ]`-выражения, IF/GOTO/THEN, WHILE/DO/END, G65/G66/G67, M97/M98/M99 (+M46/M96 — control-flow), O-коды подпрограмм, G150 (subs-геометрия), VPS-вывод — line-level `RAW(E_UNSUPPORTED_SYNTAX)`. Кросс-диалект при любом Raw — `E_RAW_UNTRANSLATABLE`. **Политика «Haas-файл с Macro B = Raw, не декомпозиция» — подтверждается** теми же аргументами, что Fanuc (§7.2 fanuc-отчёта): стек вызовов, глобальное состояние, произвольные переходы.

### 7.3 ERR-класс (жёсткий отказ, не raw)

Только **G52** (сдвиг фрейма mid-program; §2.3) — по аналогии с Fanuc-матрицей, единственный вендорский ERR. Структурные случаи (хвост-subs после program_end) — raw-хвост, не Err (правило Fanuc §7.3). Setting 33/129 делают G52-семантику Setting-зависимой — при Setting 33=HAAS `?` интерпретация может отличаться от Fanuc-стиля; v0.1 treats G52 как ERR независимо от Setting (консервативно; ревью).

---

## 8. Reverse gap: EID/L1 ЕСТЬ / Haas НЕТ или иначе (эмиссия EID → Haas)

| Узел/мета | → Haas (mill NGC) | Класс / решение |
|---|---|---|
| `state.wcs = p1..6` | `G54–G59` | 1:1 |
| **`state.wcs = p7..9`** | **`G110/G111/G112`** (слоты #7–#9) | **кандидат прямого маппинга (уникальная Haas-симметрия: EID-слоты 7–9 = нативные Haas-слоты #7–#9; G154 P7–P9 — эквивалентные алиасы)**. Эмиссия с notice; v0.1-дефолт — отказ `E_UNSUPPORTED_SYNTAX` (парс-сторона RAW до ревизии enum); ревизия этапа 12 — обоих направлений |
| `state.wcs = p>9` | кодов нет (G113–G129/#10–#26, G154 P21–P99 — вне enum) | отказ `E_UNSUPPORTED_SYNTAX`; profile-опция map → G154 Pn с notice — НЕ дефолт (аналогия `golden_preset_no_bake`) |
| `state.wcs = G59.1/.2/.3` (LinuxCNC-источник) | слоты #7–#9 — но таблица с чужим происхождением origin | отказ `E_UNSUPPORTED_SYNTAX` (правило COVERAGE §5) + profile-опция «G59.x → G110–G112» с notice (эквивалентность условна по таблицам — EQUIVALENCE §1.2) |
| wcs_set_here (G10 L20 LinuxCNC-семантика) | **Haas L20 = другое** (запись origin доп. СК) | **эмиссия запрещена как L20** → отказ `E_UNSUPPORTED_SEMANTICS` (как Fanuc; «set here» через G92 требует машинной позиции — не статично) |
| G54.1 Pn-запросы (Fanuc-источник) | кода нет; эквивалент G154 Pn — другая нумерация (P1=#7) | profile-опция map с таблицей соответствия + notice; дефолт — отказ SYNTAX |
| path_mode exact_stop | `G61` | 1:1 (значение совпадает с Fanuc) |
| path_mode exact_path | эквивалента нет (G61 = exact stop; G64 = cancel) | warning + ближайший `G61` (консервативно) ИЛИ bare-дефолт — выбор профиля (как Fanuc) |
| path_mode blend(t) | `G64` (явная отмена exact-stop) + warning; t теряется | квантификатора в словаре нет |
| dwell | `G04 P<сек.>` — **decimal point обязателен** | sub-ms dwell: X-формы нет → округление до 1 мс + notice ИЛИ отказ — ревью (Fanuc X-fallback недоступен) |
| `state.feed` (включая G93/G95-разрешения) | эмиссия всегда `G94` (EQUIVALENCE §2) | Haas G94 есть |
| arc — полное кольцо (sweep=360°, pitch=0) | `G02/G03 I_ J_` **без end-слов** | легальный Haas-формат (официальный ман); винтовой полный оборот — с end-словом оси |
| arc R-форма | всегда IJK; R — флаг профиля + notice | Haas R поддерживает; канонический контракт — IJK |
| числа | decimal point **ВСЕГДА**; округление до least increment (0.001 mm/0.0001") с notice | bare-integer в эмиссии запрещён (DPI-ловушка); 4dp-EID покрыт полностью (потерь нет — §1.2) |
| program_end | M30 (дефолт, Haas-конвенция) vs M02 — выбор профиля | auto-PEND (v0.16) → авто-вставка M30 |
| frame_preset G92 | `G92 X_ Y_ Z_` первым блоком; порядок §1.4 EQUIVALENCE | **hygiene-опция G92.1 недоступна** (кода нет): документировать — сдвиг Haas персистентен вне программы (Setting 129 `?`); отказ от G92-эмиссии при опасении загрязнения — профиль; НЕ эмитить фиктивную очистку |
| `state.tool` | `T## M06` (или M16 — профиль) | 1:1 |
| хвост M5/M9 перед PEND | не эмитить (ruling COVERAGE §5 применим) | Haas M30 сам глушит (reset-семантика) |
| циклы (развёрнутые ходы) | Haas принимает развёрнутые ходы | GRBL→Haas даст verbose — документировано (SPEC §1.2) |
| M29-эмиссия для rigid tap | **не применимо** (Haas G84 rigid внутренне) | эмиссия M29 была бы relay-выходом — опасная коллизия; в эмиссии циклов G84 не участвуем (развёртка) |

---

## 9. Рекомендации адаптеру (v0.1 / этап 10 / этап 12)

### 9.1 В v0.1 (MVP Haas-парсера, static subset)

1. **Лексер:** EOB `;` как разделитель (не комментарий!); `( )`-комментарии (парные); `%`; N-слова; decimal point обязателен к распознаванию; **bare-integer → DPI=least-increment-чтение (0.001 mm/0.0001") + notice, или `E_INVALID_INPUT`** — та же политика, что Fanuc §9.1(1) (recommendation: DPI-чтение + notice); трейлинг-точка; P-слово — DPI-домен мс (dwell).
2. **Движение:** G0/G1/G2/G3; IJK-инкремент (группы 04 нет); R-форма (±знак, R при полном кольце → `E_INVALID_INPUT`); helical; полное кольцо (§6 SPEC); **G01 с C/R-словами → RAW**.
3. **Модальности:** G17/18/19 (валидация нормали), G90/91, G20/21 (без псевдонимов!), G93/G94; G95/G60/G09 → RAW; G61 → META exact_stop; G64 → META blend (сдвиг значения vs LinuxCNC — зафиксировать в матрице).
4. **Фреймы:** G54–G59 (p=1..6), G10 L2 P1–P6, G92 (только пресетка; Setting 33/129-нюансы `?`), G53 + правило §5.3; **G10 L20 → RAW(E_REQUIRES_MACHINE_TABLE) — НЕ wcs_set_here**; G10 L2 P0/L1/L10–L13 → RAW(REQUIRES); **G52 → ERR**; G110–G129/G154 → RAW(SEMANTICS) (кандидат p=7..9 — ревизия).
5. **Циклы:** разворачивать G81/G82/G83/G73/G85/G86/G89 **по Haas-семантике** (§5.4): L-повторы (НЕ K!), P-DPI dwell, δ G73 = Setting 22 → константа 0.5 мм + notice `W_CYCLE_PARAM_ASSUMED`, переменный пекинг G83 I/J/K (d_1=I, d_k=max(K, d_{k−1}−J)), G86-рестарт, Setting 52 → notice; G84/G74/G76/G77/G156/G174/G184 → RAW; L0 `?` → не разворачивать; дефолт G99 (§7.5a SPEC); G98-требование known-position (§7.6 SPEC).
6. **Компенсации:** G43/G44/G143 → RAW(E_REQUIRES_MACHINE_TABLE); семейства: **G40 = {G41, G42, G141}**, **G49 = {G43, G44, G143}** (Haas-составы — в §1.3 COVERAGE при приёмке; Fanuc-составы не переносить).
7. **M-коды:** M0/M1/M2/M30, M3/M4/M5(+S), M6/M16(+T), M8/M9; M19/M29/M10–M13/документированное I/O → RAW(SEMANTICS); M97/M98/M99/M96/M46 → RAW(SYNTAX); M109 → RAW(RUNTIME_STATE); неизвестные M → RAW(SYNTAX) + диагностика.
8. **Macro B:** детект `#`, `[ ]`, IF/GOTO/WHILE/DO/END, G65/G66/G67, M97/M98/M99, O-коды, VPS → line-level Raw{E_UNSUPPORTED_SYNTAX}; хвост-subs → raw-хвост; кросс-диалект → `E_RAW_UNTRANSLATABLE`.
9. **Числа:** квантование 4dp (§10 SPEC); диапазоны/переполнение → `E_LIMIT_EXCEEDED`; 5dp-вход (бeyond least increment) — не встречается в словаре (least = 4dp inch) — notice не требуется.
10. **COVERAGE-приёмка:** строки §2/§3/§4 → колонка «Haas» COVERAGE.md после ревью; `?`-строки → реестр §6 COVERAGE; семейства G40/G49 Haas-составов — расширение §1.3 COVERAGE.

### 9.2 Этап 10 (профиль станка + эмиссия)

- Эмиссия EID→Haas (§8): dwell P-правило (точка обязательна), полное кольцо, числа/округление, M30, G92-hygiene (отсутствие G92.1 — задокументировать), p7–p9 → G110–G112 (кандидат маппинга), tool `T## M06`.
- G28/G29 через профиль (home-позиции известны — COVERAGE §2.3 «узел — этап 8»).
- Setting-зависимые константы (Setting 22 δ, Setting 52, Setting 57 rigid, Setting 40 comp units, Setting 58 corner style) — поле профиля станка; канон-константа δ не переопределяется (SPEC §7.4a).
- G95: разрешение при статическом S — ревизия по корпусу (совместно с Fanuc-строкой).
- G187/G103: решение RAW vs META/notice — ревью по корпусу (частотность в HSM-корпусе).

### 9.3 Этап 12 (L2-решения, не v0.1)

- Macro B / параметрика (L2): стек #1–#33, глобалы #100–#999+#10000s, IF/GOTO/WHILE, G65/G66/M97/M98-вызовы; таблица соответствия system vars Fanuc↔Haas (кросс-диалектный перенос macro).
- Трансформации: G68/G69 (поворот), G50/G51 (масштаб), G100/G101 (зеркало — climb/conventional-оговорка), G107 (цилиндрический маппинг).
- Развёртка Haas-генераторов: G12/G13 (цилиндрические карманы), G70/G71/G72 (болтовые паттерны), G47 (гравировка — при экспорте глифов), G76/G77 (при решении M19-класса: у Haas все параметры в тексте).
- WCS-ревизия: G110–G112 ↔ p=7..9 (оба направления), G154 Pn — расширение p-домена или profile-map.
- Пробинг G31/G35/G36/G37/G136; 5-осевое: G141/G143/G234/G253/G254/G255/G268/G269, G174/G184; 4/5-оси (A/B/C) и M10–M13 brakes; токарный профиль (lathe: G98/G99 = feed-режимы, G96/G97, M14/M15 и пр. — отдельная колонка).

---

## 10. Реестр `?` (сверить на этапе 3/10 по NGC manual/станку)

1. **G04 P без точки = мс** — по HelmanCNC; официальную страницу G04 (Cloudflare) сверить с manual 96-8210.
2. **Dwell P в циклах (G82/G83/G86/G89)** — та же DPI-семантика, что G04? Официальные codes-settings страницы недоступны.
3. **G83 I/J/K на mill** — официальная формулировка взята с lathe-страницы (I = первый пек, J = уменьшение, K = минимум); mill-страницу сверить.
4. **G73 I/J/K** — поддерживает ли G73 переменный пекинг или только Q.
5. **Setting 22 default** — tech doc даёт 0.02"/0.5 mm; дефолт NGC сверить (влияет на notice-политику δ).
6. **Setting 52 «G83 Retract Above R»** — поведение на mill (lathe-страница/форумы); влияет на развёртку отводов.
7. **Setting 129** — точное имя/поведение (очистка G52/G92 на M30?) и дефолт.
8. **Setting 33** — FANUC/HAAS-интерпретация G52/G92: что именно меняется; дефолт.
9. **G92/G52 общий регистр** — точная механика (взаимная перезапись, осевое разрешение).
10. **G154 Pn ↔ слоты:** P1–P20 = G110–G129 — по Haas UK; NGC-манифест слотов (#7–#105) сверить (совместимость с G-списком «G110=#7» — согласовано).
11. **G66/G67** — официальная codes-settings страница NGC существует; в quick-списке отсутствует — поддержка на NGC/CHC сверить.
12. **M46** — семантика (control-flow jump? формат Q/P).
13. **M199 без APL** — program-end-семантика.
14. **G187** — форма аргументов NGC (E float) vs CHC (P1–P4); one-shot vs persist.
15. **G167** — формат (P = setting#, Q = value?).
16. **G156 broaching** — аргументы/опции.
17. **G47** — полный список аргументов (A/B/C/D/E/I/J/R/S/X/Y/Z).
18. **G150** — точный формат (P = O-номер subs; X/Y/Z/I/D/Q/R/L).
19. **L0 в циклах** — «запись без исполнения» — сверить по manual.
20. **Дефолт G98/G99** на Haas (практика: G99; SPEC §7.5a provisional).
21. **Setting 162 дефолт OFF на NGC** — подтверждено search-снипетами; сверить по manual (влияет на DPI-политику по умолчанию).
22. **U/W-слова на mill** — Setting 162 их упоминает (lathe-наследие?); словарь mill.
23. **D-слово в G73 / R в G71** — Setting 162-исключения (lathe-контекст) — на mill не встречаются?
24. **Helical Motion Enhancement** — опция CHC vs стандарт NGC; R+Z-винтовая (R-форма с вне-плоскостным словом) — допустимость.
25. **Комментарии:** точный лимит длины по поколениям (30/40), вложенность запрещена — подтверждено; лимит NGC.
26. **M26** — в NGC-списке отсутствует (CHC-наследие? classic M26 — spindle/coolant off?) — N/A до сверки.
27. **T-предвыбор** на umbrella-changer (не SMTC) — поведение одиночного T.
28. **Power-on modal defaults NGC** — перечень для self-sufficiency-документации.
29. **G266** — формат аргументов (% выражение).
30. **M109** — формат (P = формат ввода, Q = #переменная-получатель).
31. **Диапазон чисел** (max word) — по manual (для E_LIMIT_EXCEEDED).
32. **Вложенность subs** на NGC (Fanuc = 4 уровня) — сверить.
33. **G52 group** «00 или 12» — когда 00, когда 12 (нужен ли G52-код в группе 12-контексте WCS).
34. **Setting 118** — M99-loop семантика.

---

## 11. Источники

- [Haas Mill — G-Codes (официальный список, NGC)](https://haascnc.com/service/service-content/guide-procedures/mill---g-codes.html) — полный словарь G + группы
- [Haas Mill — M-Codes (официальный список, NGC)](https://haascnc.com/service/service-content/guide-procedures/mill---m-codes.html) — полный словарь M
- [Haas Mill Operator's Manual — G-Codes (глава 17)](https://haascnc.com/service/online-operator-s-manuals/mill-operator-s-manual/mill---g-codes.html) — семантика G01 C/R, дуги (IJK-инкремент, полное кольцо), cutter comp, циклы, спец G-коды
- [Haas UK — Help with the Decimal Points](https://www.haas.co.uk/resources/better-techniques/control-features/help-with-the-decimal-points) — DPI: machinists notation по умолчанию, Setting 162 (X,Y,Z,A,B,C,E,F,I,J,K,U,W), Setting 77 (F), least increments .0001"/.001 mm
- [Haas UK — Programmable Offsets](https://www.haas.co.uk/resources/better-techniques/control-features/programmable-offsets) — G10: L2 (G52+G54–G59; P0=G52), L20 (P1–P20 → G110–G129; P1–P99 → G154), L1/L10–L13 (tool offsets), примеры
- [Haas UK — Developing Offsets](https://www.haas.co.uk/resources/better-techniques/control-tips/developing-offsets) — G154 P1–P99; G110–G129 = G154 P1–P20
- [Haas NGC Mill Operator's Manual (PDF 96-8210)](https://haascnc.com/content/dam/haascnc/en/service/manual/operator/english---mill-ngc---operator%27s-manual---2024.pdf) — первоисточник (Cloudflare-ограничен из research-среды; сверка остатка `?`)
- [Haas G84 Tapping Guide (AP-602)](https://haascnc.com/content/dam/haascnc/videos/bonus-content/ep25-tap-programming/Haas_G84_Tapping.pdf) — rigid tap = Setting 57, без M29; Q = peck tap
- [Haas Tech Doc — Drilling With High-Speed Peck (G73)](https://haascnc.com/content/dam/haascnc/ecommerce-assets/linedrawings/holemaking/modular_drill_heads/Tech_Doc_Drilling_With_High-Speed_Peck_sr_02-0060_to_02-0077.pdf) — G73 retract = Setting 22 (default 0.02"), рекомендации I/J/K
- [Haas G83 (официальная страница, lathe)](https://haascnc.com/service/codes-settings.type=gcode.machine=lathe.value=G83.html) — I = первый пек, J = уменьшение, K = минимум, L = повторы, P = dwell; Setting 22/52
- [Helman CNC — Haas G04 Dwell (сек/мс)](https://www.helmancnc.com/haas-g04-dwell-command-dwell-time-in-seconds-or-milliseconds) — P с точкой = сек, без = мс
- [Practical Machinist — Using G52 to shift work offsets](https://www.practicalmachinist.com/forum/threads/using-g52-to-shift-work-offsets.348671) — Setting 129, RESET не очищает G52
- [Practical Machinist — Haas G76 Fine Boring](https://www.practicalmachinist.com/forum/threads/haas-g76-fine-boring-cycle.357238) — Q = величина сдвига, I/J = направление
- [Practical Machinist — G86 with M19](https://www.practicalmachinist.com/forum/threads/g86-boring-cycle-with-m19.353362) — G76/G86 ориентационная механика
- [CNCCookbook — Decimal Points](https://www.cnccookbook.com/decimal-points-a-quick-way-for-the-unwary-to-crash-a-cnc-machine) — DPI-практика Haas
- [CNCCookbook — G54/G52/G53/G92 Work Offsets](https://www.cnccookbook.com/g10-g-code-fanuc-cnc-machining-haas) — G52/G92-семантика
- [GCodeTutor — G73/G83](https://gcodetutor.com/fanuc-training-course/g73-g83-drilling-cycle.html) — перекрёстная сверка пеков
- [GCodeTutor — G85 vs G86](https://www.facebook.com/GCodeTutor/posts/1544313801073960) — G86 stop/rapid-out/restart
- [Haas Mill Programming Workbook (PDF, официальный)](https://haascnc.com/content/dam/haascnc/en/service/reference/programming-workbooks/mill---programming-workbook.pdf) — L-повторы, P-dwell, структура программы
- [Haas Operator's Manual — Macros (глава)](https://haascnc.com/service/online-operator-s-manuals/mill-operator-s-manual/mill---macros.html) — диапазоны переменных, NGC-расширения
- [Zappettini Consulting — Haas G154 offsets / macros](https://zappettiniconsulting.com/learn/haas-macros/g154-offsets) — G154/G110-G129 соответствие
- [cncmacros.com — G65/G66 Guide](https://cncmacros.com/the-ultimate-guide-to-g65-and-g66-programming) — Macro B cross-check

*(Отчёт — research-артефакт этапа 10-подготовки; код не затронут, не коммитится. При переносе строк в COVERAGE.md — ревью каждой `?`-строки и Haas-составов семейств G40/G49.)*
