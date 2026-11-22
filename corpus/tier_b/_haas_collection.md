# Haas-корпус — отчёт сбора (eid-haas construction-добор)

Дата: 2026-11-22. Сборщик: агент сбора (haas-корпус, 5-й диалект).
Куда: `corpus/tier_b/_staging/` (13 файлов, НЕ inbox/ НЕ local — приёмка после
license-ревью оркестратором, CORPUS-CONTRACT §7).
Dedup: sha256 по содержимому против manifest.toml (144 файла с хэшами) —
дубликатов 0; внутри партии дубликатов 0.
Не тронуты: manifest.toml, PROVENANCE.md, crates/** (включая
eid-fanuc/tests — мутраунд D — и eid-fanuc/haas src — эмиссия-профили C),
inbox/, local/. Коммитов нет.

## 0. Префиксы источников (продолжение PROVENANCE.md)

| Префикс | Источник | Лицензия | Дата проверки | Tier-предложение |
|---|---|---|---|---|
| `zodiac_` | github.com/zodiaceng/haas-mill-programs («Custom programs used on Haas CNC Mills with Next Generation Controls») | **NOASSERTION** — /repos/license → 404, LICENSE-файла нет | 2026-11-22 | **local only** |
| `zapcon_` | github.com/ZapCon1/HaasMacros (FAST_TOOL_BREAK/) | **NOASSERTION** — /license 404 | 2026-11-22 | **local only** |
| `vultr_` | github.com/vulture2600/NPT-Macro | **NOASSERTION** — /license 404 | 2026-11-22 | **local only** |
| `coyo_` | github.com/coyoinu/HaasBAxisTram | **NOASSERTION** — /license 404 | 2026-11-22 | **local only** |
| `rensh_` | github.com/olafgromotka/HAAS-NGC-Renishaw-9000-programs (бэкап заводских Renishaw-программ NGC) | **NOASSERTION** — /license 404; сами программы — коммерческий продукт Renishaw (поставка с датчиком) → авторские права вендора, НЕ свободная лицензия | 2026-11-22 | **local only** (ценность: P-dot-rule + probing; приreject — см. §4) |
| `mpe_` | github.com/cnc4me/macro-programming-examples (examples/) — префикс уже зарегистрирован в PROVENANCE | MIT (проверено 2026-11-21, повторно не ломалось) | 2026-11-22 | published |

Пермиссивных (MIT/Apache/BSD/CC0) репозиториев с РЕАЛЬНЫМ Haas-NGC в GitHub
НЕ найдено (проверено ~15 repo-поисков + WebSearch; см. §4/§5): все живые
Haas-программы на GitHub лежат в авторских бэкапах без лицензии. Единственный
пермиссивный носитель Haas-специфики в партии — `mpe_` (MIT, Macro B
Fanuc-совместимый, диапазон #500–#699 общий).

## 1. Инвентарь ДО добора (haas в manifest 144)

Haas-специфичных файлов в корпусе было 2 (оба «incidental», обоих диалект = haas):

| Файл | Что покрывает |
|---|---|
| `tier_b/local/cncsim_g154_g43_haas_vf2.nc` (GPL-3.0, local-dev) | G154 P1/P2, G110, G111, G99 G83 Z R Q, M97 P1000, `G43 Z50. H2 M8` (Z-до-H), `.`-DPI, `G1 Y40. ,R4.` (запятая-радиус), T2 M6 |
| `tier_b/local/ncm_haas_g76_g15_eob_turn.nc` (NOASSERTION, local only) | `;`-EOB на каждой строке, G76 K D A F I P1 (Haas-токарный формат), G15, G112/G113 (lathe-polar), M154/M155, M133/M135, G50 S, G96/G97 |

Статус целевых конструкций eid-haas ДО добора:

| # | Конструкция (сверка с COVERAGE Haas-колонкой) | Было | Гэп |
|---|---|---|---|
| 1 | WCS-слоты G110–G112 (→ p=7..9), G154 P1–P3 | G110/G111/G154 P1/P2 (cncsim) | G112-mill и G154 P3 — 0 реальных сэмплов; P4+-граница — 0 |
| 2 | P-dot-rule: `G04 P1.` (сек) / `G04 P500` (мс); цикловый P (G82/G86/G89) обе формы | 0 Haas-G04 вообще (корпусные G4 P5000/P0.25 — grbl/fanuc-семантика) | обе формы в Haas-коде — дыра |
| 3 | L-повторы циклов (НЕ K): G81/G83 … L5 в G90/G91 | 0 (linuxcnc `@/^ L6` — другая семантика; `L`-повтор цикла fanuc-стиля — нет) | дыра |
| 4 | G83 переменные I/J/K (I = 1-й пек, J = уменьшение, K = минимум; без Q) | 0 (G83 только Q-форма) | дыра |
| 5 | G73 (Setting 22-клиренс) в реальном файле | 0 (G73 только в комментариях fanuc-формата) | дыра |
| 6 | M-коды Haas: M29 (relay!), M10–M13 тормоза, M13 | 0 | дыра |
| 7 | EOB `;` | ncm_haas (токарный) | фрезерный `;` — не критично |
| 8 | Tool-change идиомы: T## M06 / M06 T## / pre-call T | T2 M6 / T4 M6 (cncsim); T101 (ncm) | M06-first, переменная T, pre-call — 0 |
| 9 | Macro B Haas #500–#699 | 0 Haas-носителей (#500-класс только fanuc-mpe) | дыра |
| 10 | `G43 H# Z#.` + G154 в одном файле | cncsim (Z-до-H) | H-до-Z (`G43 H2 Z25.`) — 0 |

## 2. Собранные файлы (13, все в `corpus/tier_b/_staging/`)

Формат: файл → байт / sha256-12 → источник → license → диалект → ожидаемая
диспозиция eid-haas → конструкция.

### 2.1 Tool-breakage детекция (M-коды Haas: relay/input-класс, G154 P#, G10 L20, Macro B)

| Файл | Байт | sha256-12 | Источник | License | Диалект | Диспозиция (eid-haas) | Конструкция |
|---|---|---|---|---|---|---|---|
| `zodiac_toolbreak_m59_g154pvar.nc` | 1573 | `d13f77cc5d14` | raw .../zodiaceng/haas-mill-programs/HEAD/Haas Mill Tool Breakage Detection.NC (O08000) | **NOASSERTION** | haas-mill NGC | **M59/M69 ×4 → Raw(SEM)** (output relay, класс M29-коллизии/LinuxCNC-M62); **M96 P10 Q17 ×2 → Raw{SYNTAX}** (external-input-alarm); **G154 P#5** (#5=98. → P4+ → Raw(SEM) «слот вне EID p-домена»); **G10 L20 P#5 G90 X#10585 Y#10586 Z#10582 → Raw{TABLE}** (переменная-P в L20!); `G43 H#3026 Z.2` (H-до-Z) → Raw(TABLE); `G0 G53 Z0.` → Ok; GOTO/N100/#3000 → Raw{SYNTAX}; skeleton G0/G1/M5/M9/M30/% → Ok | 6 (relay!), 1 (P4+-граница), 8 (G10 L20 P#), 9 (macro), 10 |

### 2.2 FAST TOOL BREAK серия (G31, G4 P с точкой, G103, TSC M-коды, G154 P10/P11)

| Файл | Байт | sha256-12 | Источник | License | Диалект | Диспозиция | Конструкция |
|---|---|---|---|---|---|---|---|
| `zapcon_g31_tsc_o9613.nc` | 1606 | `151624c7959e` | raw .../ZapCon1/HaasMacros/main/FAST_TOOL_BREAK/O9613-COOLANT-1.nc (O09613) | **NOASSERTION** | haas-mill NGC | **G31 Z#27 F ×2 → Raw(SEM)** (Feed Until Skip — runtime-позиция; G31-сирота прошлого раунда закрыта!); **`G4 P5.` (DWELL FOR TSC) → NODE(dwell) сек-чтение** (точка); **G103 P1 ×5 → Raw(SYN)** (не в словаре NGC — generic-ведро); **M89 → Raw(SEM)** (TSC off); M98 P9799 ×2, G65 P9855/P9856, `#5021–#5063`, `#3004`, IF/GOTO/#3000 → Raw{SYNTAX} (Macro B); skeleton → Ok | 2 (P-dot), 6 (M89), 9 (macro-vars, machine-pos #5021+), G31 |
| `zapcon_g81_p0_g154p10_drill_ex.nc` | 605 | `096e2e00bf3a` | raw .../FAST_TOOL_BREAK/6. O09613-DRILL-EX.nc (O00200) | **NOASSERTION** | haas-mill NGC | **`G81 G98 Z1.5 Q0.1 F30. R2. P0.` → цикловый P-dot-форма (P0.)!** — dwell-слово в цикле, P0 → drop/notice-политика (сверить при приёмке); **`G00 G90 G154 P10 X0. Y0.` → P10 = слот #16 → Raw(SEM) «вне p-домена»** + WCS+оси в одном блоке; `T33 M06`, `G43 H33 Z2.` (H-до-Z!) → Ok-узел/Raw(TABLE); G53-пары `G00 G90 G53 Z0. M05` → Ok; Q0.1 в G81 (недокументированное слово в G81 — реальный шум постпроцессора, tolerance-тест); M01 → Ok | 2 (цикловый P), 1 (P4+-граница), 10 (H-до-Z), 8 |
| `zapcon_g65_ij_g154p11_endmill_ex.nc` | 639 | `d24d7568fba1` | raw .../FAST_TOOL_BREAK/7. O09613-ENMDILL_EX.nc (O00201) | **NOASSERTION** | haas-mill NGC | `G154 P11` ×2 → Raw(SEM); `G65 P9613 H0.01 I0.25 J0.` (аргументы H/I/J) → Raw{SYNTAX}; `T42 M06`, `G43 H42 Z2.`; G53-пары; M30 с комментарием-хвостом | 1, 5→9 (macro-args), 10 |

### 2.3 NPT-резьба G152 (Haas-спеццикл, M97 L-выражение)

| Файл | Байт | sha256-12 | Источник | License | Диалект | Диспозиция | Конструкция |
|---|---|---|---|---|---|---|---|
| `vultr_g152_npt_pipe_example.nc` | 445 | `139a723910d6` | raw .../vulture2600/NPT-Macro/HEAD/Pipe threading example.nc | **NOASSERTION** | haas-mill NGC | **G152 ×2 → Raw(SEM)** (Haas NPT Thread Mill — спеццикл, вне словаря COVERAGE); **ловушка: строка `G152 I2.81 J8 Z-.8 E50. F40. D22)` — закрывающая `)` без открывающей** (реальный шум файла — unclosed-paren tolerance); `G0 G43 H22 Z.3 M8` (H-до-Z, `.`-DPI); `T22 M6`; **G60 → Raw(SYN)** (в словаре NGC-mill отсутствует); G53 Z0./Y0. M30 → Ok | 6 (G152 — редкий Haas-G), 8, 10 |
| `vultr_m97_l_macro_o9016.nc` | 2456 | `322890638e53` | raw .../vulture2600/NPT-Macro/HEAD/NPT MACRO.nc (O09016) | **NOASSERTION** | haas-mill NGC | **`M97 P301 L [ [ ABS[ #26 ] * #5 ] + 1 ]` → L-повтор ПОДПРОГРАММЫ с выражением** (единственное живое Haas-L-доказательство в партии; цикловый L — сирота, §4); M97 → Raw{SYNTAX} (control-flow); `IF [ #4 EQ #0 ] #3000= 1` (alarm-idioma ×4); `#[ 2400 + #7 ] + #[ 2600 + #7 ]` (чтение offset-таблиц — Raw); `G03 G91 I [ #2 / 2 ] X#2 F#9`; G152-определение-аргументы I/J/D/E/F/Z; `#5= #5 * 10000` (bare-int-фикс DPI) | 3 (L-выражение — частично), 9 (Macro B: #3003/#3001/#3000, аргументы I/J/D/E/F/Z) |

### 2.4 Renishaw NGC-программы (P-dot-rule обе формы, G154 P98, T#3026 M06, probe)

| Файл | Байт | sha256-12 | Источник | License | Диалект | Диспозиция | Конструкция |
|---|---|---|---|---|---|---|---|
| `rensh_g04p_dot_warmup_o09220.nc` | 690 | `af40b4ae72f7` | raw .../olafgromotka/HAAS-NGC-Renishaw-9000-programs/HEAD/O09220 WARMUP PROGRAM.txt | **NOASSERTION** (вендорская — Renishaw/Haas-стиль) | haas-mill NGC | **`G04 P200.` ×6 → NODE(dwell), точка = СЕКУНДЫ** — эталонная dot-форма (200 с); `S500 M03 … S10000 M03` (ступенчатый прогрев); M30; `%`; skeleton → Ok. Самый чистый dot-носитель партии | 2 (dot-форма!) |
| `rensh_g04p_bare_xyz_o09811.nc` | 3171 | `bf5510b7eeb3` | raw .../O09811_RENISHAW_Storm.txt (O09811 XYZ MEASURE) | **NOASSERTION** | haas-mill NGC | **`G04 P1` ×7 + `G04 P250` ×1 → беz-точки = МИЛЛИСЕКУНДЫ** (+notice по политике v0.9) — парная к dot-форме; **G103 P1 ×2; G31 X Y F → Raw(SEM)**; G65 P9724/P9725/P98xx ×8; `WHILE [...] DO1 … END1`; `#5041/#5042/#5043`; `#3001 LT 200` (graphics-тест); IF/GOTO-лестница #3000= 91; комментарий-носитель `(G04 P1.0)` | 2 (обе формы в одной программе + сосед), 9, G31 |
| `rensh_g65_probe_bore_o09351.nc` | 814 | `df518b5aa84c` | raw .../O09351_RENISHAW_Storm.txt (O09351 IPS BORE RECT) | **NOASSERTION** | haas-mill NGC | `G04 P1`×10/`P250` bare + G103 P1 ×2; **`G43 H#3026` (переактивация H без Z — «G43 без Z»-форма)**; **`G65 P9814 D#7 R#18 Q#17`** (D/R/Q-аргументы probe-цикла); `#161= 556` + `#[ #161 ]` indirect; `#190= #3 + #5041` (mixed); G31 X Y F | 2, 9, 10 (G43-без-Z), G31 |
| `rensh_g154p98_tbd_o09006.nc` | 701 | `63d18362e23b` | raw .../O09006.txt (O09006 HIGH SPEED TBD CYCLE) | **NOASSERTION** | haas-mill NGC | **`G00 G154 P98 A0.` → P98 = верх слота #26 → Raw(SEM)** (+ A-слово в том же блоке); **`T#3026 M06` → переменная-T в tool-change** (NODE(tool_change), T-выражение — вне статического подмножества соседней строки); `G91 G31 Z-0.1 F [ 500. * #179 ]` (G31 в G91!); `G43 H#3026 Z [ #28 + 0.05 ]` (H-до-Z с выражением); `M98 P9854 (DELAY)`; `#26= #[ 2000 + #20 ]` (offset-таблица); G65 P9799/P9855/P9856 ×4; G103 P1 ×3; `IF [ #27 GE #24 ] GOTO9000` + THEN-строка | 1 (P98-граница), 8 (переменная T), 10, G31 |
| `rensh_settings_check_g103p1_o09725.nc` | 2430 | `d8184a0f3172` | raw .../O09725_RENISHAW_Storm.txt (O09725 SETTINGS CHECK) | **NOASSERTION** | haas-mill NGC | **`IF[#1 EQ #0] THEN #598= 1.` → THEN-форма присваивания** (≠ GOTO-форма — отдельная ветка Macro B); **`#598` — живой #500–#699 (non-volatile)-носитель ×66 `#5xx`-хитов**; `#20045`, `#30057.23`/`#30057.25` (bit-addressing системных переменных); `G103P1` (без пробела!); ladder `IF [ … ] GOTO10…N80` ×8 | 9 (#500–#699!), THEN-форма, G103-без-пробела |

### 2.5 B-axis tram (probe-идиомы, T## M06)

| Файл | Байт | sha256-12 | Источник | License | Диалект | Диспозиция | Конструкция |
|---|---|---|---|---|---|---|---|
| `coyo_tram_b_probe_o09817.nc` | 584 | `82b8cbeabea0` | raw .../coyoinu/HaasBAxisTram/HEAD/TRAM_B.nc (O09817) | **NOASSERTION** | haas-mill NGC | `T19 M06` (T## M06-идиома); **`G43 G55 H19 Z2.` (G43+G55+H+Z в одном блоке!)**; `G65 P9832/P9810/P9817/P9833` ×5 (Renishaw probe-макросы — реальная версия стиля mpe_g65_macro_centerline_probe); `G00 B0`, `G00 B#10189` (ротационная ось B-словом + выражение — Ok/Raw по осевому набору профиля); G28 Z10.; M00 → Ok | 8 (T## M06), 10 (комбинированная G43-строка), B-ось |

### 2.6 Macro B #500–#699, пермиссивный носитель (MIT)

| Файл | Байт | sha256-12 | Источник | License | Диалект | Диспозиция | Конструкция |
|---|---|---|---|---|---|---|---|
| `mpe_toolpath_profiling_m1.nc` | 1181 | `e9417c8cb2c8` | raw .../cnc4me/macro-programming-examples/main/examples/TOOLPATH_PROFILING.NC | **MIT** | fanuc Macro B (Haas-совместимый поверхность) | `#3001 = 0`, `#540 = 551` → **#540 в диапазоне #500–#699 (non-volatile)**; `#[#540] = #3001` (indirect-двойное); `#[#540] / 1000`; `T1 M6` ×3 → Ok; **M1 (optional stop) ×3 → Ok**; skeleton → Ok. Единственный пермиссивный файл партии | 9 (MIT-носитель диапазона) |

## 3. Итог покрытия ПОСЛЕ добора

| # | Конструкция | Было | Стало (staging; +2 существующих local) | Закрыто? |
|---|---|---|---|---|
| 1 | WCS-слоты G110–G112 / G154 P1–P3 | G110/G111/G154 P1/P2 (cncsim) | +3 границы: G154 P10, G154 P11, G154 P98/P#5 (все → Raw(SEM) «вне p-домена» — негативная граница ABSORB-правила P1–P3) | **ЧАСТИЧНО**: P1–P3 и P4+-границы покрыты; **G112-mill-слот и G154 P3 — сироты** (G112 в корпусе — только lathe-polar) |
| 2 | P-dot-rule G04: `P10.`=сек / `P10`=мс | 0 Haas | **ОБЕ ФОРМЫ**: `G04 P200.` ×6 (o09220, dot), `G4 P5.` (zapcon), `G04 P1`/`P250` bare ×8+11 (o09811/o09351) — dot и bare в РАЗНЫХ реальных программах | **ДА** (обе лексические формы) |
| 2b | Цикловый P (G82/G86/G89) обе формы | 0 | `G81 … P0.` (dot-форма, zapcon_drill) | **НЕТ** — только одна форма в одном файле; G82/G86/G89-P — сироты |
| 3 | L-повторы циклов (G81/G83 L5) | 0 | `M97 P301 L [выражение]` (L-повтор подпрограммы) | **НЕТ** — цикловый L — сирота; L-выражение M97 — частичное доказательство L-слова |
| 4 | G83 I/J/K переменные пеки | 0 | +0 | **НЕТ — сирота** (только tutorial-страницы) |
| 5 | G73 (Setting 22) | 0 | +0 | **НЕТ — сирота** (только tutorial-страницы) |
| 6 | M-коды Haas (M29-relay, M10–M13) | 0 | **M59/M69 ×4** (output relay — семейство M29-коллизии!), **M96 P Q ×5** (external-input alarm), **M89** (TSC), M00 | **ЧАСТИЧНО**: relay-семейство закрыто M59/M69 (аналог M29-relay); **литеральный M29 и M10–M13 — сироты** (M13-гипотеза уже опровергнута в COVERAGE) |
| 7 | EOB `;` | ncm_haas (lathe) | +0 (новые файлы — `%`-стиль без `;`) | ДА (остаётся ncm_haas) |
| 8 | Tool-change идиомы | T2 M6 / T4 M6 / T101 | **`T#3026 M06`** (переменная-T!), `T19 M06`, `T22 M6`, `T33/T42 M06` | **ЧАСТИЧНО**: T## M06 + переменная-T закрыты; **M06-first и pre-call (T один) — сироты** |
| 9 | Macro B Haas #500–#699 | 0 Haas | **#540–#553 (MIT mpe), #598 ×66-хитов (o09725), #163–#174/#167–#179 (zapcon/o09006)** + THEN-форма, bit-addressing #30057.25 | **ДА** (+ THEN/GOTO/WHILE/indirect-поверхность) |
| 10 | `G43 H# Z#.` + G154 | cncsim (Z-до-H) | **H-до-Z формы**: `G43 H33 Z2.`, `G43 H42 Z2.`, `G43 H#3026 Z.2`, `G43 H#3026 Z [expr]`, `G43 G55 H19 Z2.`, + G43-без-Z (`G43 H#3026`) | **ДА** (все порядки слов, включая ранее не найденный H-до-Z) |
| — | G31 (сирота прошлого раунда) | 0 | **G31 ×7 вызовов** (Z/XY-формы, G91-форма, F-выражения) | **ДА** (Raw(SEM)-класс с живой поверхностью) |

**Итог: 5/10 конструкций закрыты (2, 7, 9, 10 + G31-бонус), 4 — частично
(1, 3, 6, 8), 2 — полностью без источников (4: G83 I/J/K, 5: G73).**
Значительный побочный выигрыш: негативные границы WCS (P10/P11/P98),
M59/M69-relay-семейство, переменная-T в M06, unclosed-paren шум.

## 4. Сироты — конструкции БЕЗ публичного корпусного доказательства
(проверено repo-поисками GitHub + WebSearch, 2026-11-22):

1. **G83 I/J/K (переменные пеки)** — существуют только официальные страницы
   haascnc.com (Cloudflare, сбор запрещён) и gcodetutor-туториалы; ни одного
   .nc/.tap с I/J/K-формой в GitHub не найдено.
2. **G73 в реальном файле** — та же картина: tutorial-only. (В корпусе G73
   встречается только в комментариях fanuc-формата `G73U9W-0.1R6`.)
3. **Цикловый L-повтор** (`G91 G98 G81 Z R L8` haas-стиль) — не найдено;
   Fanuc-аналоги — K-форма (в словаре Haas K ≠ повтор!). M97 L-выражение —
   частичное доказательство L-слова.
4. **G82/G86/G89 с P (обе DPI-формы)** — найден только `G81 … P0.` (dot);
   dwell-P в G82/G86/G89 реальными файлами не подтверждается.
5. **G112 mill-WCS-слот и G154 P3** — в реальных файлах не встречены
   (cncsim_g154 закрывает P1/P2+G110/G111; G112 в корпусе — lathe-polar
   ncm_haas, иная семантика).
6. **M29 (Haas relay-литера), M10–M13 (тормоза осей)** — не найдено ни одного
   .nc; семейство прикрыто M59/M69 (тот же output-relay класс, что M29-Haas).
7. **M06 T## (M06-first) и pre-call T-одно-слово** — не найдено; все файлы
   партии — T## M06-порядок.
8. **G66/G67 (модальный macro-вызов)** — только в fanuc-словарях; Haas-файлов
   с G66/G67 не найдено.
9. **M13/M19-mill** — M13 отсутствует в Haas mill-словаре (опровергнуто в
   COVERAGE v0.9.4 — подтверждено отсутствием и в собранной партии);
   M19-orient встречен только в fanuc-lathe cncsim (вне haas-зоны).

## 5. Пропущенные источники (license-блок / не собрано)

| Источник | Причина |
|---|---|
| haascnc.com / CNCCookbook / gcodetutor / helman (G73, G83-IJK, G04-док) | официальные страницы и туториалы — Cloudflare/©, verbatim-сбор запрещён (правило раунда) |
| MechaPhoton/GCODE (CC0-1.0) | содержимое — LLM-псевдо-G-code (`#num_holes = 8`, именованные переменные, `;`-комменты mid-line) — НЕ Haas-NGC, teaching-мусор для парсера |
| bivekSapkota/Generate_GCODE_For_HAAS_Using_LLM | license отсутствует + LLM-датасет (синтетика) |
| fbadalj/nc-programs (HAAS VF-2) | license отсутствует; единственный файл 3.7MB (> cap 600KB) и без уникальных конструкций |
| olafgromotka/Python-Renishaw-NGC-postprocessor, coyoinu-форки | генераторы логики, не NC |
| franknaw/G-Code-Examples | license отсутствует (NOASSERTION) — уже отбракован прошлым раундом |
| ZapCon1 «6./7.» варианты учтены (605/639 байт — взяты); прочие Renishaw-программы (O09301, O09815–O09856, O09982…) | НЕ взяты сознательно: коммерческий вендорский продукт Renishaw (поставка с датчиком), распространяемый бэкапом без лицензии — взяты только 4 файла минимального достатка (P-dot, G154 P98, THEN/#598, probe-идиомы); при reject — конструкции P-dot-rule остаются без dot/bare-пары (см. §6) |

## 6. Приёмочные пометки

- Файлы `rensh_*` — **вендорский Renishaw/Haas-контент** (бэкап программ,
  поставляемых с покупкой датчика). NOASSERTION по §7; если оркестратор
  отклоняет даже local-only — минимальное ядро партии, которое стоит
  сохранить из иных источников: dot/bare-P пару (`o09220`/`o09811`) и
  `o09006` (G154 P98 + T#3026 M06). Остальное заменяемо.
- `zapcon_*`/`zodiac_*`/`vultr_*`/`coyo_*` — авторские бэкапы без лицензии,
  «local only» при ценности (правило раунда).
- `mpe_toolpath_profiling_m1.nc` — MIT → может идти в published/inbox.
- Ловушки для eid-haas, добавленные партией: unclosed-paren (`vultr_g152`),
  G103P1 без пробела (`o09725`), Q-слово в G81 (zapcon_drill — постпроцессорный
  шум), `G00 G154 P98 A0.` (WCS+осевой блок), THEN-присваивание, `#[ 2400 + #7 ]`
  offset-чтение, `#30057.25` bit-addressing.
- Партии: 1 партия, 13 файлов, sha256 полные посчитаны (таблицы выше —
  12-символьные префиксы), dedup против 144 = 0, коммитов нет,
  manifest/PROVENANCE/inbox/local/crates не тронуты.
