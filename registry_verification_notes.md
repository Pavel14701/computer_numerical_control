# REGISTRY-VERIFICATION-NOTES — сверка `?`-реестров COVERAGE §6 (этап 3)

**Агент:** D. **Дата:** 2026-11-22. **Зона:** COVERAGE §6 (+шапка v0.9.4), этот файл. Крейты не тронуты, коммита нет.

**Метод:** web-сверка (официальные источники → secondary → форумы) + source-check кода контроллеров. Каждому факту присвоен уровень источника:
- **OFFICIAL** — документация вендора (haascnc.com online manual/print PDF 2023, haas.co.uk) или source-код контроллера (gnea/grbl).
- **MEDIUM** — специализированные справочники (HelmanCNC, GCodeTutor, Zappettini — secondary, со ссылками на первоисточники).
- **WEAK** — форумы (Practical Machinist, CNCZone, blog-статьи) — помечено, на снятие `?` в поведенческих решениях не тянет.

**Первичные источники этого прохода (все fetch 2026-11-22):**
- NGC Mill Operator's Manual 2023 (полный print-текст, OFFICIAL): https://haascnc.com/content/dam/haascnc/en/service/manual/operator/english_mill_interactive_manual_print_version_2023.pdf — главы: 12 Programming, 13 Macros, 16 Options (VPS), 17 G-Codes, 18 M-Codes, 19 Settings. Онлайн-зеркала глав: `haascnc.com/service/online-operator-s-manuals/mill-operator-s-manual/mill---g-codes.html`, `.../mill---m-codes.html`, `.../mill---macros.html`, `.../mill---settings.html`.
- Haas UK (OFFICIAL-зеркало): Help with the Decimal Points (`haas.co.uk/resources/better-techniques/control-features/help-with-the-decimal-points`), Programmable Offsets (`.../programmable-offsets`), Developing Offsets (`.../control-tips/developing-offsets`).
- grbl source (OFFICIAL): `raw.githubusercontent.com/gnea/grbl/master/grbl/{protocol.c,gcode.c,nuts_bolts.c}`.
- HelmanCNC: Fanuc 30i G-code list; Fanuc DPI (`/fanuc-decimal-point-programming-introduction-parameters`); Haas G04 dwell.
- GCodeTutor: subprograms M98/M99 (`gcodetutor.com/cnc-macro-programming/subprograms-m98-m99.html`).
- Zappettini Consulting wiki: canned cycles (`zappettiniconsulting.com/wiki/canned-cycles` — ссылки на Smid «Fanuc CNC Custom Macros» 2005 + Fanuc Operator's Manual).
- Practical Machinist: Fanuc G76 (тред 282543), G52/Setting-129-тред (348671) — WEAK.
- IndustrialMonitorDirect blog (K0 на 31i) — WEAK/MEDIUM.

---

## 1. GRBL source-check — §6.68 (SpaceInWord)

**Вердикт: наш eid-grbl КОНВЕРГЕНТЕН с реальным grbl — НЕ баг. Гипотеза «наш лексер слишком мягок» ОПРОВЕРГНУТА.**

1. `protocol.c` (protocol_main_loop, приём строки по символам):
   ```c
   if (c <= ' ') {
     // Throw away whitepace and control characters  (опечатка в оригинале)
   } else if (c == '/') {
     // Block delete NOT SUPPORTED. Ignore character.
   ```
   Любой символ ≤ 0x20 (включая пробел) **выбрасывается до** записи в line-buffer → в `gc_execute_line()` пробелы не доходят никогда.
2. `gcode.c` (gc_execute_line, ядро слова) — строго:
   ```c
   letter = line[char_counter];
   if((letter < 'A') || (letter > 'Z')) { FAIL(STATUS_EXPECTED_COMMAND_LETTER); }
   char_counter++;
   if (!read_float(line, &char_counter, &value)) { FAIL(STATUS_BAD_NUMBER_FORMAT); }
   ```
   `read_float` стартует с символа сразу после буквы.
3. `nuts_bolts.c` `read_float()` — ведущие пробелы НЕ пропускает (первый символ должен быть `+/-/./digit`; «No spaces assumed in line»).

**End-to-end:** `G0 X 10.5 Y20` реальный grbl исполняет как `G0X10.5Y20` — принимает. Наш лексер (`crates/eid-grbl/src/lexer.rs:110-112` — пропуск `is_whitespace()` между буквой и числом) — то же поведение. Формулировка COVERAGE §4 «GRBL — парсер строг» верна для ядра, но end-to-end grbl пробелы принимает — ячейку синхронизировать (вне зоны §6-правки).

**Fanuc-часть §6.68** — не закрыта (manual-сверка лексики недоступна) — `?` остаётся. **Haas-часть (§6.69)** — `?` остаётся: grbl-прецедент показывает, что ядро-парсер и end-to-end поведение могут различаться — переносить вывод на Haas нельзя.

## 2. Haas NGC — официальные подтверждения (manual 2023 + haas.co.uk)

### G91.1/G90.1 отсутствие — top-priority, ячейка §2.2/§5.2
- G-список ch.17.1 (полный, с группами): arcs = только `G02/G03 (Group 01)`; группы листа: 00,01,02,03,05,06,07,08,09,10,11,12,15,16,23 — **группы 04 нет, кодов G90.1/G91.1 нет**.
- ch.17.2: «The I J K values define the **relative** X Y Z distances from the starting point to the center of the circle. ... This is also the only method to cut a circle. Defining only the I J K values and not defining the end point X Y Z values will cut a circle.» — центр всегда инкрементальный от start; полное кольцо = I/J/K без end-слов.
- **Следствие:** строка §2.2 «Haas: группы 04 нет — I/J/K инкрементальные от start всегда (G90.1/G91.1 в словаре нет — как GRBL)» — подтверждена ОФИЦИАЛЬНО.

### G110–G129/G154 слоты — top-priority (p=7..9-симметрия)
- ch.17.1 (G-список, дословно): `G110 #7 Coordinate System 12` … `G112 #9 Coordinate System 12` … `G129 #26 Coordinate System 12`; `G154 Select Work Coordinates P1-P99 12`.
- ch.12.3: «Work Offsets available are G54-G59, G110-G129, and G154 P1-P99. **G110-G129 and G154 P1-P20 refer to the same Work Offsets.**»
- Macro-таблица ch.13.5: «#7001-#7006 G110 (G154 P1) additional work offsets; #7021-#7026 G111 (G154 P2); #7041-#7386 G112 - G129 (G154 P3 - P20); #14401-#14406 G110(G154 P21); #14421-#15966 G154 P22-P99».
- Haas UK Developing Offsets: «G110 to G129 refer to the same work offsets as the G154 P1 through P20»; «provision for storage of 26 work offsets, namely G54-G59, and G110 to G129».
- **Следствие:** ABSORB p=7..9 (G110–G112 = слоты #7–#9) — подтверждено напрямую; ячейки §2.3/§5.2 — в силе.

### G52 группа — «00 или 12» дословно
- ch.17.1: `G52 Set Work Coordinate System — 00 or 12` (буквально); ch.8.5 повторяет «G52 Set Work Coordinate System (Group 00 or 12)».

### G92/G52 — регистр
- ch.13.5: `#5201-#5206 G52 work offsets` — отдельного диапазона для G92 в таблице переменных НЕТ.
- ch.8.5: «G92 Set Work Coordinate Systems **Shift Value** (Group 00)» — на той же Work Offset странице.
- WEAK (CNCZone 162349): «The G92 shift value is displayed at the bottom of the Work Offsets page and may be cleared there». Точная механика перезаписи — forum-level, `?` сужен.

### G50/G51
- ch.17.1: `G50 Cancel Scaling 11` / `G51 Scaling 11`; ch.16.2/ch.17.5: «You must purchase the rotation and scaling option»; «G51 applies a scaling factor ... G50 cancels scaling».

### Setting 77 / 162 (DPI — top-priority)
- Haas UK DPI (OFFICIAL): «Haas control engineers have addressed this issue by adding two specific settings — setting 77 and setting 162 ... setting 77 is solely for the feedrate command, whereas setting 162 will apply to address codes **X, Y, Z, A, B, C, E, F, I, J, K, U, W**».
- Setting 77: «F12 becomes: **0.0012units/minute with Setting 77 Off**, 12.0 units/minute with Setting 77 On»; таблица масштабов: «INCH **DEFAULT (.0001)** ... F1 = F.0001; METRIC **DEFAULT (.001)** ... F1 = F.001».
- Setting 162: «When this setting is On, the control will add a decimal point to values ... When the setting is **Off**, values following address codes that do not include decimal points are taken as **machinists notation** (i.e., thousandths or ten-thousandths.)»; примеры: inch `X-2 → X-.0002` (Off), mm `X-2 → X-.002`.
- Исключения 162: «**A (except with G76)** — If a G76 A value containing a decimal point is found ..., **alarm 605 Invalid Tool Nose Angle** / **D (except with G73)** / **R (except with G71 in YASNAC mode)**».
- NGC settings ch.19 (имена): `77 Scale Integer F`, `162 Default To Float`. Буквальной строки «factory default = OFF» ни в одном fetched-тексте нет (оговорка к снятию §6.53), но Off-поведение = machinists notation документировано как нотация Haas, а 77-таблица прямо помечает «DEFAULT (.0001)/(.001)» — наш §4 (least increment 0.001mm/0.0001"; F12 = 0.0012 units/min; Setting 77/162 дефолт-чтение) подтверждён.
- **U/W** — в 162-домене OFFICIAL (см. выше) → §6.54 снято (в части «Setting 162 их упоминает»); motion-словарь mill не меняется.
- **Исключения** — все три lathe-контекст → §6.55 снято (на mill не возникают).

### G04 P — dot-rule
- OFFICIAL ch.13.1 (look-ahead пример): «`#12012 = 1 ; G04 P1. ; #12012 = 0 ;` This is intended to turn an output on, **wait 1 second**, and then turn it off».
- OFFICIAL ch.16.8 (probe): «`M59 P2 ; G04 P1.0 ; M59 P3 ;` This turns on tool probe communication, **delays one second**».
- MEDIUM (Helman Haas G04): «G04 P10. (is a dwell of 10 seconds, see decimal at the end)» / «G04 P10 (is a dwell of 10 milliseconds. No decimal point)»; X/U-форм на странице нет.
- G82 (ch.17.4): «The optional argument **Pn.nnn** specifies the duration of the dwell» — dotted-форма в циклах.

### Циклы
- ch.17.4 (mill!): G83 — «Peck depth can be **variable or constant and always incremental**. Qnn.nnn. **Do not use a Q value when programming with I, J, and K.**» → §6.35 снято.
- G73 — «The G73 High-Speed Peck Drilling Canned Cycle is **the same as the G83** ... except that tool peck retraction is specified with **Setting 22 - Can Cycle Delta Z** ... The initial peck depth, defined by **I**, should generally be a depth of 1 tool diameter.» → §6.36 снято (I/J/K на G73 подтверждены).
- G86 — «The G86 Bore and Stop Canned Cycle is the same as the G85 ... except that the **spindle will stop at the bottom** of the hole before returning». G89 — «there is a dwell at the bottom».
- L-повторы: ch.12.2 пример (OFFICIAL): «`N13 G99 G91 G81 F8.15 X1.25 Z-0.3 L5 ;` ... G81 uses the loop address code, Lnn ... The address code **L5** repeats this process (5) times. Each time the canned cycle repeats, it moves the distance that the optional X and Y values specify» — G91+L-механика официально. L0 — в тексте нет (форумы WEAK: «skip hole»/data-load) → §6.51/70 `?` остаётся.
- G98/G99 (ch.17.4 R Planes): «G98 ... moves the Z axis to the height of the Z axis prior to the canned cycle. G99 ... to the height specified by the Rnn.nnnn» — **дефолт не назван** → §6.52 `?` остаётся.

### Setting 129 — ОПРОВЕРЖЕНИЕ атрибуции
- Официальный settings-лист (ch.19; online + 2023 print PDF): `...119 Offset Lock, 120 Macro Var Lock, **130 Tap Retract Speed**...` — **Setting 129 отсутствует, диапазон 121–129 пуст целиком**.
- CHC-карты (search-снипет, MEDIUM): «M50 Execute Pallet Change (**Setting 121 thru 129**)» — в classic-списках 121–129 = APC/pallet-позиции, не G52.
- PM-тред 348671 (WEAK, дословно): «On a Haas, **you can change a setting where M30 will reset G52**, but in no time will RESET change it on it's own» — **номер 129 в треде не назван**. G92: HuFlungDung: «subsequent G92's accumulate in Haas» vs CNCZone «cleared automatically» — конфликтуют.
- **Следствие:** «Setting 129 = очистка G52/G92 на M30» — неверная атрибуция (§6.39/§6.72); семантика очистки на M30 — не верифицирована. Ячейки §2.3 (G52/G92/G92.1), §5.2 (program_end «(Setting 129 ?)») — кандидаты синхронизации формулировок.

### Setting 33 / 118 / 22 / 52
- ch.19 (имена, OFFICIAL): `33 Coordinate System`, `118 M99 Bumps M30 Cntrs`, `22 Can Cycle Delta Z`, `52 G83 Retract Above R`, `56 M30 Restore Default G`, `81 Tool At Power Up`, `191 Default Smoothness`, `27 G76 / G77 Shift Dir.`.
- **Setting 118 — наша формулировка «M99-loop семантика (число циклов/условие)» неверна**: официально это инкремент счётчиков M30 при M99-лупах → §6.66 закрыто с уточнением; строка §3 M98/M99 — кандидат синхронизации.
- **Setting 27 «G76 / G77 Shift Dir.»** — направление сдвига G76/G77 на Haas может быть setting'ом — конфликтует с нашей §2.5/§7.16 («направление задаётся I/J-словами в тексте» из PM-треда, WEAK) — **флаг на ревью** (не влияло на диспозицию RAW(SEM), влияет на «кандидат этапа 12: все параметры в тексте»).

### M-коды (ch.18, OFFICIAL)
- `M29 Set Output Relay with M-Fin` (+ M129, M59/M69) — **M29-relay подтверждён** (top-priority; коллизия с Fanuc M29 rigid-tap в силе, эмиссия M29 → Haas запрещена — §5.2).
- `M46 | **Qn Pmm Jump to Line**` → §6.44 снято.
- `M199 Pallet / Part Load or Program End` → §6.45 снято (по имени кода; APL-условие — supplement, не вычитано).
- **M26 отсутствует** в официальном списке (последовательность M21-M25 → M29 → M30) → §6.58 снято (documented absence, N/A подтверждена).
- M97/M98/M99 — как в матрице; «Only one M-code is allowed per line of code. All M-codes take effect at the end of the block.»
- M109 Interactive User Input — имя подтверждено; формат P/Q не вычитан → §6.62 `?` сужен.

### Macro B / subs
- ch.13.8: «NOTE: **G00, G65, G66, and G67 cannot be aliased.**» → §6.43 снято (NGC-присутствие G66/G67).
- ch.13.7: «G65 calls can be **nested up to 9 times**» → §6.64 частично (M97/M98-глубина не задана).
- ch.12.4: «The local subprogram format is to **end the main program with an M30 then enter the local subprograms after the M30**» — хвост-subs паттерн официально (контекст §6.28/§4 O-слов).
- ch.13.3 таблица аргументов G65: A→#1, B→#2, C→#3, D→#7, E→#8, F→#9, H→#11, I→#4, J→#5, K→#6, M→#13, Q→#17, R→#18, S→#19, T→#20, U→#21, V→#22, W→#23, X→#24, Y→#25, Z→#26 — **идентично Fanuc-маппингу** (наш §4 Macro B Haas «аргументы A–Z → #1–#33 — Fanuc-совместимый ?» — подтверждено; `?` был в §4, вне зоны правки).
- Macro-переменные: глобальные #10000–#10999 вкл. legacy #100-199/#500-699/#800-999; «factory-installed options use global variables, for example, **probing and pallet changers**» (#550-#599 = probe calibration) — подтверждает §4 «часть #800-#999 зарезервирована (probes, pallets)».

### Tool offsets / M06
- ch.8.3 (OFFICIAL): «**By default tool offsets will now always be applied**, unless a G49/H00 (Mill) ... is explicitly specified. **On mills, when a tool change occurs, the tool offset will automatically update to match the new tool.**»
- ch.12.3: «**Setting 15 - H & T Code Agreement** controls whether the nn value needs to match in the Tnn and Hnn ... Alarm 332 - H and T Not Matched».
- ch.10.4 (SMTC): «the control looks ahead as far as **80 lines** ... When look-ahead finds a tool change, the control puts the next tool in your program into position. This is called "tool pre-call."» пример: `T01 M06 ; T02 (PRE-CALL THE NEXT TOOL) ;` → §6.59 частично (SMTC подтверждён; umbrella ch.9 — pre-call не описан).
- **Следствие-флаг:** ячейка §3 M6 Haas «M6 не применяет TLO (применяет G43) — restart ?» противоречит ch.8.3 для NGC — кандидат ревизии (вне зоны §6-правки).

### Прочее официальное
- ch.17.1 группы: G61 Exact Stop Mode **15**, G64 G61 Cancel **15**; G68/G69 **16**; G70/G71/G72 bolt-паттерны **00**; G31/G35/G36/G37 **00**; G141 **07**; G143/G234 **08**; G254/G255 **23**; G268/G269 **02**; G266/G167/G187/G107/G103/G100/G101/G47/G150 **00**; G156 Broaching **09**; G174/G184 **00**; циклы G73-G89 **09**.
- ch.17.2: «**G40 cancels cutter compensation and is the default condition when you power on** your machine»; «Setting 58 ... Available Setting 58 values are Fanuc or Yasnac».
- ch.5.2: «On power-up, this position uses **work offset G54** automatically».
- ch.17.5: G150 «uses a subprogram ... X-Y starting point in the G150 command is within the boundary ... Alarm 370 - Pocket Definition Error».
- ch.16.3 VPS G47-пример: «`G47 E7.5000 F15. I45. J.5 P0 R0.05 Z-0.005 (TEXT TO ENGRAVE)`»; таблица: P = Text/Serial switch (0/1), I = Text Angle, J = Text Height, R = R-Plane, Z = Depth.
- ch.4.6 (клавиатура): «End-of-block (EOB) ;», «Parentheses (, ) ... must always be entered as a pair» — EOB-ловушка §4 подтверждена.
- ch.13.2: «Enter a value (**maximum is 999999.000000**)» — лимит ввода macro-переменной (не word-домен; §6.63 не закрывает).

## 3. Fanuc — web-сверка

### K/L, K0, M98-слип
- Zappettini canned-cycles (MEDIUM; refs: Peter Smid «Fanuc CNC Custom Macros» 2005 + Fanuc Operator's Manual): «Number of times to repeat the cycle, used with G91 incremental positioning to drill an equally-spaced row. **K0 positions without drilling.** ... **Fanuc uses K; some controls use L.**» → §6.16 снято (повторы = K), §6.17 частично (K0-семантика).
- GCodeTutor M98/M99 (MEDIUM): «M98 P52000;» — «The number that precedes the program number is the amount of times it is to be repeated (in this case 5 times)» → слип-формат подтверждён; «Some versions of FANUC approach this subject slightly differently ... **M98 P2000 L5**»; «Up to **4** subprograms can be nested» → §6.22 снято.

### G76 / 5101
- PM тред 282543 (WEAK/MEDIUM — точная ссылка на manual): «The manual I'm looking at says you change **parameter bits 5101.4 and 5101.5** ... See page 338 of Parameter Manual (**B-63530EN/03**)»; «you set the shift direction with a parameter since **the control ignores a negative Q**»; «Q (shift at the bottom of a hole) is a Modal value». → §6.18 остаётся `?` по P/P2 (30i-manual недоступен), биты 5101 подтверждены вторично.

### DPI
- Helman DPI (MEDIUM): «parameter (**No.3401#0** or No.2400#0)» — 3401 на 30i/0i/16i/18i/21i; «**0**: The least input increment is assumed. (Normal decimal point input)»; «**1**: The unit of mm, inches, degree, or second is assumed»; calculator-type = «Calculator-type Notation». Заводской дефолт бита — не заявлен → §6.27 `?` сужен.

### Dictionary-absence (Helman 30i полный список, MEDIUM)
- G90.1/G91.1 — **отсутствуют** (G91.1 занят «Checking the maximum incremental amount specified» — сомнительная статья Helman); G92.2/G92.3 — **отсутствуют** (только G92/G92.1); G61.1 — **отсутствует** (G61 есть); G09, G50/G51 — есть (группы лист не даёт).
- → §6.25 снято (documented absence, RAW(SEM) обоснован), §6.23 посылка опровергнута для 30i, §6.15 `?` остаётся (противоречие Helman-списка с «опцией на 30i» отмечено).

### G98/G99 дефолт
- CNCCookbook G98/G99 (MEDIUM): дефолт не назван → §6.21 `?` остаётся.

## 4. Не-верифицируемые позиции (классификация инвентаризации)
- **Policy/review (не web):** §6.26 (IS-C 5dp — решение ревью), §6.30 (G95 ABSORB — ревизия по корпусу).
- **Код-кандидаты (не web):** §6.28 (last-O-subs детект), §6.71 (Fanuc R-хвост — этап 12).
- **Machine-specific (unresolvable-remote):** §6.29-остаток (M06-restart), §6.15 (16i/0i-F manual), §6.17-остаток (B-61863), §6.19 (parameter manual δ), §6.21 (0i manual), §6.24 (Fanuc group tables), §6.27-остаток (3401#0 default), §6.38-остаток, §6.39/72-остаток (станок), §6.40-остаток, §6.41-остаток.
- **Unresolvable-today (Cloudflare/страниц нет):** §6.56 (Helical UK), §6.57 (комментарии-лимит), §6.63 (max word), §6.69 (Haas-лексер), §6.46-49/50/61/62-остатки (форматы аргументов detail-страниц).

## 5. Флаги синхронизации ячеек §2–§5 (вне зоны §6-правок, для следующего прохода)
1. §4-строка SpaceInWord: «GRBL — парсер строг» → end-to-end grbl пробел принимает (§6.68).
2. §3 M6 Haas: «M6 не применяет TLO (применяет G43) — restart ?» → ch.8.3: offsets auto-update при tool change + Setting 15 H&T.
3. §2.3/§5.2 «Setting 129 ?» (G52/G92-очистка, program_end) → Setting 129 не существует (§6.39/72).
4. §2.5/§7.16 Haas G76 «направление = I/J в тексте» → официальный Setting 27 «G76/G77 Shift Dir.» (§6.18-флаг) — **ЗАКРЫТ v0.9.5 (Pass B, §6 ниже)**.
5. §3 M98/M99 «Setting 118 ?» → Setting 118 = «M99 Bumps M30 Cntrs» (§6.66).
6. §2.2 G61.1 Fanuc-ячейка → G61.1 отсутствует в 30i-списке (кандидат N/A, §6.23).
7. §4 Macro B Haas «аргументы A–Z Fanuc-совместимый ?» → подтверждено (ch.13.3, маппинг 1:1 с Fanuc).
8. §2.5 G73 Haas: «переменные пеки I/J/K на G73 ?» → подтверждено (ch.17.4).
9. (Pass B, v0.9.5) §4-строка «I/J/K-контекст (Haas)»: «направление сдвига G76 (I/J)» — верно только для I/J-формы G76 (без Q); Q-форма несёт направление в Setting 27 (см. §6.1 ниже) — кандидат синхронизации формулировки, вне зоны правок Pass B.

## 6. Проход Agent B (v0.9.5) — закрытие двух открытых вопросов

**Агент:** B. **Дата:** 2026-11-22. **Зона:** COVERAGE §2.5-строка G76 / §7.16 / §6.18-флаг / §6.71 (+шапка v0.9.5), этот файл, CHANGELOG v0.9.5. Крейты не тронуты (eid-fanuc/eid-haas — только чтение для локализации код-гэпа), коммита нет, RAW/Err-диспозиции не менялись.

### 6.1 Q1: Haas G76 — Setting 27 vs I/J-слова — ВЕРДИКТ (OFFICIAL)

**Источники (оба fetch 2026-11-22):**
- **OFFICIAL — codes-G76 detail-страница** (впервые прошла Cloudflare; прежние проходы — «detail-страницы недоступны»): https://haascnc.com/service/codes-settings.type=gcode.machine=mill.value=G76.html
- **OFFICIAL-зеркало — Mill Programming Workbook** (в списке источников с v0.9; сниппет из индекса PDF совпадает дословно с codes-страницей): https://haascnc.com/content/dam/haascnc/en/service/reference/programming-workbooks/mill---programming-workbook.pdf
- **WEAK (согласовано):** PM тред 357238 — «setting 27 controls the direction that the tool backs off» / «There is a parameter setting for g76 move off direction, might be #27 on your machine».
- Онлайн-глава ch.17 (mill---g-codes.html) G76 описывает без механики сдвига («bores the hole to the specified depth and after boring the hole, moves to clear the tool from hole before retracting») — механика живёт на codes-detail-странице.

**Дословные цитаты (codes-G76):**
- Word-таблица: «**I** - Shift value along the X-Axis before retracting, **if Q is not specified** / **J** - Shift value along the Y-Axis before retracting, **if Q is not specified** / **Q** - The shift value, **always incremental**».
- Тело: «In addition to boring the hole, this cycle will shift the X and/or Y Axis prior to retracting in order to clear the tool while exiting the part. **If Q is used Setting 27 determines the shift direction. If Q is not specified, the optional I and J values are used to determine the shift direction and distance.**»
- Workbook-зеркало: «The Q value shift direction is set with setting 27. If Q is not specified, the optional I and J values can be used to determine shift direction and distance.»

**Вердикт — механизмы взаимоисключающие (XOR по наличию Q), все три прежних сценария сняты:**
1. (а) «Setting 27 выбирает ось, I/J задают величину» — НЕВЕРНО: I/J читаются ТОЛЬКО при отсутствии Q.
2. (б) «Setting 27 полностью заменяет I/J» — НЕВЕРНО: I/J-форма — полная альтернатива (направление И величина в тексте).
3. (в) «I/J для G76, Setting 27 только для G77» — НЕВЕРНО: Setting 27 = «G76 / G77 Shift Dir.» — для обоих.
4. ФАКТ: **Q present →** величина = Q («always incremental»), направление = **Setting 27** (вне текста программы); **Q absent →** опциональные **I/J** = направление И величина (I = X-компонента, J = Y-компонента).

**Следствие для этапа 12:** кандидат «все параметры в тексте» — **снят в общей форме, сужен до I/J-формы**. Q-форма несёт направление в Setting 27 — вне текста (по сути Fanuc-5101-класс); отличие Haas от Fanuc сохраняется как «наличие текстовой альтернативы», но не как «всё в тексте». RAW(SEM) не меняется (M19-класс ориентации на дне в силе; CNCARENA-тред — «G76 … to position the spindle in the proper place before the Z retract» — weak-согласовано).

**Остаток `?`:** домен значений Setting 27 (перечень вариантов направления: X+/X-/Y+/Y-?) — не вычитан: detail-страница Setting 27 (codes-settings.type=setting…value=27.html) не рендерит документ (2026-11-22) — unresolvable-today, на механику/диспозиции не влияет.

### 6.2 Q2: Fanuc G10 L10/L11 — R-слово — ВЕРДИКТ (формат: ДА, R — data-слово формы)

**Источники (fetch/поиск 2026-11-22):**
- **MEDIUM — cncmacros.com** (полный гид по G10; в списке источников COVERAGE с v0.8): https://cncmacros.com/blog/cnc-macro-variables/fanuc-g10-command-programming-the-complete-guide-to-offsets-parameters-and-safe-automation
- **WEAK — CNCZone тред 95572** («G10 L10 P10 R5;» — пример практиков).
- Helman/Zappettini G10-страницы в этом проходе не добавили ничего сверх cncmacros (L10-формат у них уже покрыт L2-фокусом).

**Дословные цитаты (cncmacros):**
- «**L10** – Set tool length offsets (**Geometry**) / **L11** – Set tool length wear offsets (Wear) / **L12** – Set tool diameter offsets (Geometry) / **L13** – Set tool diameter wear offsets (Wear) / **L110** – Set corner radius offset (Geometry) / **L111** – Set corner radius offset (Wear)».
- Синтаксис: «**Memory A** = G10 (L11) P_n_ **R_n_** / **Memory B** = G10 (L10/L11) P_n_ **R_n_** / **Memory C** = G10 (L10 - L13, L110, L111) P_n_ **R_n_**»; «**R_n_ values: R = Tool compensation value (R.625, R5.253)**»; пример «**G10 L10 G90 P5 R2.5** (Set Tool 5 length = 2.5) / G10 L12 G90 P5 R0.375 (Set Tool 5 diameter = 0.375 absolute)».

**Вердикт:** R — **обязательное data-слово официального Fanuc-формата** записи таблицы инструментов (`G90 G10 L1x P_ R_`); какую ячейку пишет R — определяет L-номер. **Нюанс формулировки исходного вопроса: в L10 R несёт GEOMETRY-значение (длину), НЕ радиус** — радиус/диаметр = L12/L13 (тоже R-словом); corner radius = L110/L111. Пример haas-этапа 2 `G10 L10 G90 P5 R250.` = geometry-длина смещения #5 — потребление R на Haas корректно.

**Код-гэп локализован (чтение крейтов, НЕ правка):**
- eid-fanuc `canonicalize.rs` unused_word_diagnostic, Frame-план, DataRecord-ветка: `consumed.extend(['P', 'L'])` — **R не потребляется** → строка `G10 L10 G90 P5 R250.` на шаге (14) unused-word получает catch-all **Err(E_INVALID_INPUT)** «R word without consumer in block (arc/cycle/G10/G4 context expected)» и НЕ доходит до шага (17) G10-классификации Raw{E_REQUIRES_MACHINE_TABLE} — вопреки ячейке матрицы (Fanuc L10/L11 = RAW(TABLE)) и официальному формату.
- eid-haas `canonicalize.rs` Frame/DataRecord: `consumed.extend(['P', 'L', 'R'])` — потребляет (этап 2 eid-haas) — асимметрия адаптеров.

**Кандидат-правка (бриф агенту этапа 12/N, НЕ реализовано):** в eid-fanuc unused_word_diagnostic Frame/DataRecord-ветке добавить `'R'`: `consumed.extend(['P', 'L', 'R'])` (зеркало eid-haas) → вся G10-строка → **Raw{E_REQUIRES_MACHINE_TABLE}** (свой матричный класс; R поглощается как data-слово формы — «молчаливый пропуск» отсутствует: слово потреблено G10-формой). Тест-кандидат: `g10_l10_r_tool_table_raw` (строка официального формата → Raw{RT}, без Err). Замечание: это реальное изменение поведения адаптера (Err → Raw) для класса строк — поэтому код-кандидат этапа 12, а не hotfix; COVERAGE-ячейки не меняются.

## 7. Применимость emission-профилей этапа 8 (2a365cc) к grbl/linuxcnc — анализ (НЕ код)

**Агент:** D. **Дата:** 2026-11-22. **Зона:** этот файл (+ заметка-наблюдение в EQUIVALENCE.md §4 — отдельная задача). Крейты не тронуты, коммита нет. Активные версии: EQUIVALENCE v0.3.1, SPEC v0.18.1, COVERAGE v0.9.5.

**Метод:** чтение кода профилей (без правок): `crates/eid-integration/src/profile.rs` (validate_emission_flags:367–421; ProfileEmissionOpts), `crates/eid-integration/src/translate.rs` (guard 640–658; with_emission_opts 102–114), эмиссия program_end (`eid-grbl/src/emit.rs:624`, `eid-linuxcnc/src/emit.rs:669`), профили TOML (`profiles/grbl_default.toml`, `profiles/linuxcnc_default.toml` — оба уже несут `emit-r-arcs = false`, валидный no-op). Применимость — по уже сверенному словарю: COVERAGE §2.1/§2.3/§3/§5 (вкл. grbl source-check §1 этого файла), EQUIVALENCE §4/§5. Web-сверка не потребовалась — все словарные факты верифицированы прежними проходами.

**Факты о текущей механике (2a365cc, чтение кода):**
1. `validate_emission_flags`: `program_end_style = M02` и `wcs_extended_map = Some` на диалектах, отличных от fanuc/haas, → отказ загрузки профиля («тихое игнорирование запрещено»); **`emit_r_arcs` диалект-гейта НЕ имеет**.
2. Translate-гвард: не-дефолтные опции при `dialect профиля ≠ цель` → отказ; при совпадении диалекта опции кладутся в обёртку, но **grbl/linuxcnc-обёртки опции не получают вовсе** (`with_emission_opts`: Fanuc/Haas only — trait v0.2 заморожен, `emit_with_opts` у этих адаптеров нет).
3. Эмиссия program_end на grbl/linuxcnc — захардкоженная **M2** (grbl: «requires M2 (CORPUS n14)»); serde-дефолт флага = M30.

### 7.1 Таблица флаг × диалект → вердикт

| Флаг | → grbl 1.1 | → LinuxCNC | Вердикт |
|---|---|---|---|
| `program_end_style` (M30\|M02) | Применим технически: M2 и M30 — оба `NODE(program_end)` (COVERAGE §3; grbl README); различие M2/M30 вне L1 (EQUIVALENCE §5). Но **не нужен**: эмиссия L1-эквивалентна любым из двух кодов, диалектной ловушки нет (≠ Haas M30 с reset-семантикой); текущий байт-дефолт — M2 захардкожен, а serde-дефолт флага = M30 → plumbing с дефолтом «как у fanuc/haas» СМЕНИЛ бы байт-дефолт цели (золотовые R1-тексты) — против инварианта 2a365cc «дефолт emit байт-в-байт по построению» | То же: M2/M30 оба `NODE(program_end)`; эмиссия = M2 (`emit.rs:669`) | **НЕ НУЖЕН** (обе цели принимают оба кода — вопрос задачи снят: флаг не несёт L1-различия). Текущий validate-отказ M02 корректен как защита от тихого игнорирования (опции до адаптера не доходят), но **текст ошибки неверен по существу**: «в словаре … флага program_end нет» — оба кода в словаре ЕСТЬ; настоящая причина — механика опций не проброшена + дефолт M30 ≠ байт-дефолт M2. Кандидат переформулировки сообщения (текст, не поведение). Если plumbing когда-нибудь понадобится — обязателен dialect-dependent дефолт (M2) |
| `wcs_extended_map` | **НЕПРИМЕНИМ — целевых кодов нет.** Посылка вопроса «G59.1-.3 ЕСТЬ в словаре grbl» — **ОПРОВЕРГНУТА**: COVERAGE §2.3 = N/A (нет в словаре); G59.1–.3 — это **grblHAL-дельта** (§6.2 реестра, `?`), не vanilla grbl; grbl 1.1 = ровно G54–G59 = EID p1..6. Мапе некуда мапить: слоты p7..9 не представимы, G59.x-эмиссия и так отказ `E_UNSUPPORTED_SYNTAX` (§5) | **НЕ НУЖЕН — избыточен:** G59.1/.2/.3 = `ABSORB` нативно (§2.3), эмиссия `state.wcs = G59.x` → «ок» напрямую (§5). Флаг неприменим: цель имеет **нативные расширенные WCS** — расширенные слоты AST (p7..9) отображаются 1:1 без таблицы подмены | **Неприменим обоим**; validate-отказ верен. Формулировка для документации: «grbl — нет целевых кодов; linuxcnc — нативные G59.1–.3, мапа избыточна». Оговорка: grblHAL/FluidNC — pending-адаптеры; при их реализации (G59.1–.3 заявлены у grblHAL, §6.2) linuxcnc-вывод переносим, grbl-вывод нет — не переносить автоматически |
| `allow_r_arcs` (`emit_r_arcs`) | Применим **технически**: R-форма в словаре (COVERAGE §2.1 «R и IJK → канон §6»; source-check §6.9/§6.11 реестра). Противоречия профиля и канона НЕТ: EQUIVALENCE §4 прямо санкционирует R-эмиссию «только по флагу профиля с notice» — флаг и есть санкционированное исключение из канона always-IJK | То же (R-форма легальна; helical-R документирован — §6.9) | **Технически применим, НЕ рекомендован** (дефолт false сохранять): R-эмиссия повторно вводит ill-conditioned R→центр-амплификацию A(sweep) при re-parse (motivation always-IJK в EQUIVALENCE §4: повторное 4dp-квантование R даёт ложнокрасный на околополуокружностях; `golden_semicircle_R`); эквивалентностной пользы нет. **НАЙДЕН ГЭП согласованности:** в отличие от M02/map, `emit_r_arcs = true` на grbl/linuxcnc-профиле **проходит validate** и **тихо игнорируется** (опции не пробрасываются в адаптер; same-dialect транслит и translate-гвард не ловят — гвард срабатывает только при dialect≠цель). Это против собственного принципа этапа 8 «тихое игнорирование запрещено» |

### 7.2 Ответы на вопросы задачи (сводно)

1. **program_end_style:** оба кода в словарях обеих целей → флаг технически возможен, но **обе цели принимают оба кода и флаг не нужен** (различие вне L1 — EQUIVALENCE §5/COVERAGE §3; пользы fanuc/haas-типа «вендор-конвенция» нет, дефолт M30 конфликтовал бы с байт-дефолтом M2).
2. **wcs_extended_map:** флаг **неприменим**: grbl — нативных расширенных WCS нет вообще (посылка «есть в словаре grbl» ошибочна — COVERAGE §2.3), linuxcnc — нативные G59.1–.3 делают мапу избыточной. Задокументированная формулировка: «флаг неприменим: цель имеет нативные расширенные WCS / не имеет целевых кодов».
3. **allow_r_arcs:** R-эмиссия legal в обоих диалектах (словарь), флаг применим технически и **не является противоречием профиля и канона** (EQUIVALENCE §4 санкционирует его с notice), но рекомендация — не включать: канон always-IJK защищает R1-эквивалентность (амплификация A(sweep), ill-conditioned near-semicircle). Противоречие обнаружено не в паре «флаг-канон», а в валидации: R-флаг — единственный из трёх, не гейтящийся по диалекту → тихий no-op на grbl/linuxcnc.

### 7.3 Рекомендации для брифа следующей итерации (НЕ реализовано)

1. **Emission-профили для grbl/linuxcnc (пункт §10 REPORT): закрыть констатацией «флаги неприменимы/не нужны»** — новые ключи в `grbl_default.toml`/`linuxcnc_default.toml` не вводить (оба профиля уже валидны с `emit-r-arcs = false` и дефолтами; отсутствие ключей M02/map = семантический no-op по построению serde-default). Plumbing опций в grbl/linuxcnc-адаптеры — только при реальном кейсе (для program_end_style потребовал бы dialect-dependent дефолта).
2. **Кандидат-фикс (одна проверка, behavior-change только на не-дефолтных TOML):** зеркалить гейт `emit_r_arcs` в `validate_emission_flags` (не-fanuc/haas + true → отказ загрузки, текст «флаг неприменим: R-эмиссия не пробрасывается в адаптер …»), устраняя тихий no-op. Дефолтные профили не затрагивает (false валиден везде).
3. **Кандидат переформулировки текста ошибки** `program_end_style`-validate («в словаре … флага program_end нет» → «опции этапа 8 механически не пробрасываются в адаптер …; в словаре цели оба кода program_end есть — различие вне L1»): текст диагностики, поведение не меняется.
4. В COVERAGE/брифе зафиксировать формулировки §7.1 (wcs_extended_map: «неприменим: grbl — нет целевых кодов / linuxcnc — нативные G59.1–.3») как ответ на вопрос применимости — без правки ячеек матрицы (они уже согласованы: §2.3/§5).
