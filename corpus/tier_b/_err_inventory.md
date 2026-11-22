# ERR-инвентаризация Tier B — классификация всех 81 Err свипа (SPEC v0.19)

**Дата:** 2026-11-22 (агент C, итерация инвентаризации)
**Вход:** `corpus/tier_b/_sweep_results.txt` (157 файлов, ok=76, err=81; классы: E_INVALID_INPUT=40, E_RUNTIME_STATE=36, E_UNSUPPORTED_SEMANTICS=4, E_UNSUPPORTED_SYNTAX=1)
**Метод:** группировка по КОРНЮ причины (диагностическое сообщение + разбор файла), затем вердикт по трём источникам: (а) реальный контроллер (GRBL/LinuxCNC source+docs, зонд-выкладки COVERAGE, web-сверка констант), (б) SPEC v0.19 / COVERAGE v0.10, (в) вердикт: **[REAL-SEM]** (контроллер исполняет, мы отказываем — кандидат) / **[TRUE-ERR]** (реальный контроллер тоже откажет) / **[AMB]** (неоднозначно, нужна проба/профиль-решение).
**Точность:** для каждого геометрического/структурного класса место отказа подтверждено `eid-cli verify` (строка/колонка в span-диагностике); ключевые гипотезы проверены зонд-файлами (`target/probe_c_inv/`, см. §3). crates/COVERAGE/manifest не менялись.

**Дизъюнкция с агентом D:** три класса — G92-mid (`grbljs_sample.gcode`), end-not-last (`mit_hilbert_test.ngc`, `vis_knight.tap`), bare-T (`vince_BewareOfDog_v10_2_T2.nc`, `vince_BewareOfDog_v10_3_T4.nc`) — **зона D-зондов, ждём** (не классифицируются этим отчётом).

---

## 1. Сводная таблица классов (корень причины → файлы → вердикт)

| # | Класс (корень) | n | Код | Файлы | Реальный контроллер | SPEC v0.19 | Вердикт |
|---|---|---|---|---|---|---|---|
| 1 | `#`-параметр/переменная (параметрика) | 30 | E_RUNTIME_STATE | mpe_g65_macro_centerline_probe/deep_drill/material_verify, mpe_toolpath_profiling_m1, mpe_while_clear_variables, gmoccapy_macros_change, g33_threading, 3D_Chips, daisy, flowsnake, gridprobe, ngcgui_lib arc1/arc2/backlash/db25/gosper/lathe_g76base/lathe_g76diam, nurbs Curva ×4, probe_basic ×3, rensh_g04p, rensh_g65, vultr_m97, zapcon_g31, zodiac | LinuxCNC/Haas **исполняют** (RS274NGC-параметрика, Macro-B — ядро обоих) | §1.1: отказ; Q10 закрыт — параметрика = этап 12 (L2) | **[REAL-SEM]** — L2-locked, см. тир B (§4) |
| 2 | word-без-числа = слово перед параметром/выражением | 7 | E_INVALID_INPUT | coyo (B#10189), gmoccapy_go_to_position (X#1), increment (x#1), cone (параметрика+while+оси B/C/W), rensh_g154p98 (T#3026), good-arc.big imperial+metric (I[expr]) | LinuxCNC **исполняет** (то же ядро-язык; good-arc — файлы из его собственного тест-набора `tests/interp/good/`) | то же §1.1 | **[REAL-SEM]** — L2-locked, тир B |
| 3 | Motion внутри Raw-блока не двигает цепочку позиций | 5 | E_INVALID_INPUT | comp311 (`g41 g1 y4` → радиус-мисматч на :27), cncsim_g84_g43_placa (`G41 D2 G1 X0 Y-10` → chord>2R на :31), ncm_g41_d_2d5 (`G41 X60.5 Y95.5 D3` → chord>2R на :653), ncm_g434_rtcp_5x (`G0 A/C`-блок Raw → голые координаты), ncm_g43_g69_3d5x (A/C/G43-блоки → радиус-мисматч на :1867) | **исполняют** (все 5 — валидные файлы; comp311 — официальный пример из Handbook LinuxCNC) | артефакт канонизатора, не SPEC-правило | **[REAL-SEM]** — кандидат v0.20 №2 (§4) |
| 4 | Допуск радиуса дуги (N5a-проверка строже контроллера) | 2 | E_INVALID_INPUT | vis_hse1p (δ=0.0011 на :37), grbljs_Circle_Diamond_Square_Calibration (δ=1.414 на :624) | GRBL: **проверки радиуса IJK-дуги нет вообще** (принимает оба); LinuxCNC: допуск TOLERANCE_MM=0.005 / INCH=0.0005 (interp_internal.hh) → vis_hse1p (mm) проходит, calib нет | §6.1 N5a: tol = max(0.001, 1e-6·ρ) (eid-grbl canonicalize.rs:1089) | vis_hse1p **[REAL-SEM]** (оба контроллера принимают); calib **[AMB]** (grbl-да/LinuxCNC-нет, δ велик — профиль-решение; AMB-счётчик) |
| 4a | Lathe-дуга в радиус-семантике (diameter-mode) | 2 | E_INVALID_INPUT | ncm_g87_g88_g121_turn1 (:176 `G3 X100. Z-181. R.5`), ncm_haas_g76_g15_eob_turn (:174, тот же ход) | Токарь (Fanuc/Haas, диаметральная X по умолчанию): хорда = 0.707 ≤ 2R=1.0 — **исполняет**; зонд G18 подтвердил: в радиус-семантике хорда 1.118 > 1.0 | §1.2: токарная семантика — вне скоупа v0.0 (этап 10) | **[REAL-SEM]** — но фикс = diameter-mode (этап 10), НЕ v0.20-L1 |
| 5 | Лексический мусор — вложенные скобки в комментарии | 9 | E_INVALID_INPUT | gcb ×8 (`by` в ASCII-арте), gce_g71_bareint_testg71 (`mm`) | rs274 **отказывает** («Nested comment found» — зонд rs274, аудит #17, COVERAGE §4); Fanuc — вложенность запрещена маном | §4 COVERAGE: LinuxCNC → E_INVALID_INPUT, «диспозиция честна» | **[TRUE-ERR]** |
| 6 | Лексический мусор `GOTO` (Macro B) | 1 | E_INVALID_INPUT | gce_full_industrial_lathe.g (:382, :396) | Fanuc **исполняет** (GOTO N — Macro B); COVERAGE Macro-B: GOTO → line-level RAW(SYNTAX) | политика line-Raw существует (v0.16), но лексер не токенизирует GOTO → падает раньше | **[REAL-SEM]** — кандидат v0.20 (лекс-толерантность → Raw{SYNTAX}) |
| 7 | Control char \u{1} в строке | 1 | E_INVALID_INPUT | mit_cws_fresa.gcode (:98 ` y58.669\x01`) | GRBL **принимает**: protocol.c выбрасывает все байты `c <= ' '` ДО парсера (source-check gnea/grbl, COVERAGE §4 «пробел внутри слова») | лексер строже GRBL | **[REAL-SEM]** — кандидат v0.20 (grbl-лексер: strip c ≤ ' ') |
| 8 | Линейный ход без F | 2 | E_INVALID_INPUT | mit_pycnc_circles (:4 `G1 Z10` до `f1800` на :5), mit_pycnc_test_parser | GRBL **отказывает** (STATUS_GCODE_UNDEFINED_FEED_RATE), LinuxCNC тоже | §3/§4: linear требует feed — соответствует | **[TRUE-ERR]** |
| 9 | Дуга-продолжение без offset-слов | 1 | E_INVALID_INPUT | grbljs_full-circle-without-xyz (:11 `N55 X.75` — модальная G2 без IJK/R; мы занулили offset → ρ_start=0 vs ρ_end=0.25) | GRBL **отказывает** (error 2 STATUS_GCODE_ARC_OFFSETS_MISSING — IJK/R обязательны в КАЖДОМ дуговом блоке), LinuxCNC — «I/J word missing with G2/G3» | §6 — вердикт совпадает; текст диагностики улучшить (у нас «radius mismatch», причина = offset missing) | **[TRUE-ERR]** |
| 10 | P-слово без потребителя (после NURBS-фикса) | 2 | E_INVALID_INPUT | arc-feed tests: arc_maxvel_xy (:6 `G2 … P4`), arc_time (:7/:14 `G2 … P2`) | rs274 **отказывает** («P word with no G4/G10/G64/G76/G82/G86/G89» — P не потребляется G2) | v0.19: NURBS-P → Raw снял ложные срабатывания на 11 файлах butterfly/leto/nurbs/trident — остаток ТОЛЬКО эти 2 | **[TRUE-ERR]** |
| 11 | Переключение единиц в середине программы | 3 | E_UNSUPPORTED_SEMANTICS | lcnc_m70_m72_demo (:24 g21), lcnc_m73_demo (:34 g21), lcnc_nc_files_g76 (:36 g21 после g20 на :5) | LinuxCNC **исполняет** (конверсия на лету — G20/G21 легальны в любой позиции) | §2.5 принцип 5: одна система единиц — структурный Err (правило двух классов: raw-сохранение исказило бы последующие опы) | **[REAL-SEM]** — кандидат только через ревизию §2.5 (тир C: convert-at-read) |
| 12 | Частичный end после разрыва (b) (WCS-свитч) | 1 | E_RUNTIME_STATE | cncsim_m98_m99_duas_morsas (:12 `G55 G0 X-20. Y-20.` — Z наследует неизвестное) | LinuxCNC **исполняет** (двух-тиски программа; origins в таблице — операторский контракт, COVERAGE §0.3) | §2 (v0.11): после genuinely-unknown частичный end → E_RUNTIME_STATE | **[REAL-SEM]** — тир C (схема: per-axis unknown / Z-hold) |
| 13 | Partial G92 (не все оси) | 1+2 | E_RUNTIME_STATE | mit_cws_torno (`G92 z100`, XZ-токарный); mit_ddp_eck3, mit_ddp_m_retract (корень свипа тот же, см. строку ниже) | GRBL/LinuxCNC **принимают** partial G92 (offset только названных осей) | frame_preset(vec3) — схема не представляет частичную пресетку | torno **[REAL-SEM]** — тир C (per-axis frame_preset); **но**: eck3/m_retract — RepRap-файлы с E-словами (`G92 E0`, `G1 … E0.0983`) → **[TRUE-ERR]** (в реальном GRBL E — не слово, отказ независимо от G92) |
| 14 | Дублированный program end | 1 | E_INVALID_INPUT | mit_tball_tball-engrave-laser (:3455 второй M2/M30) | GRBL/LinuxCNC **не ошибаются** — останавливаются на первом M2/M30, хвост игнорируют | §3: program_end ровно один, последний | **[REAL-SEM]** — кандидат v0.20 (truncate-at-first-PEND + notice; зеркало auto-PEND v0.16) |
| 15 | M6 без T внутри O-sub (remap-макрос) | 1 | E_INVALID_INPUT | gmoccapy_macros_change_g43 (:14 M6 в теле `o<change_g43> sub`) | LinuxCNC **принимает** файл (тело sub не исполняется при чтении; T подставляет remap-механизм) | M6 → NODE(tool_change) требует T | **[REAL-SEM]** — кандидат v0.20 (Raw-зона O-sub: не валидировать процесс-коды внутри тела) |
| 16 | G53 без G0/G1 в кадре | 1 | E_INVALID_INPUT | vultr_g152_npt_pipe_example (:21 `G53 Z0.`, :22 `G53 Y0. M30`) | LinuxCNC **отказывает** (документировано: «It is an error if: G53 is used without G0 or G1 in the same block») | соответствует | **[TRUE-ERR]** |
| 17 | Цикл как первый motion-оп | 1 | E_RUNTIME_STATE | zapcon_g81_p0_g154p10 | реальный контроллер **исполняет** (initial Z — текущая позиция) | §7.6: развёртка недетерминирована → отказ | **[AMB]** — SPEC-политика; кандидат возможен только с решением «initial Z = unknown» (родственно #12) |
| 18 | Machine-frame ход при внешнем origin WCS | 1 | E_RUNTIME_STATE | zapcon_g65_ij_g154p11 (G154 P11 — Haas-слот #11 вне EID p-домена; G53-ход) | Haas NGC **исполняет** (слот из таблицы) | §5.3(b): machine-frame требует origin in-text — намеренное исключение | **[AMB]** — политика §5.3; файл = Haas-диалект под linuxcnc-меткой (расширенный WCS — Raw-класс) |
| 19 | Голые координаты со старта (Excellon-контент) | 1 | E_INVALID_INPUT | mit_shpk_ncdrill1.tap (86 строк `X..Y..`, ни одного G-кода; формат Excellon 2.x implied-decimal) | GRBL: дефолтная G0-мода → «исполнил» бы как **мусорные** рапиды (X01785 = 1785!); LinuxCNC: дефолтная G1 + нет F → отказ. Файл — не G-код вообще | §3 явная модальность | **[AMB]** — не кандидат: семантика сломана даже у «принявшего» контроллера |
| 20 | IF (control flow) | 1 | E_UNSUPPORTED_SYNTAX | rensh_settings_check_g103p1 | LinuxCNC **исполняет** (o-if/Haas-стиль) | §1.1 SYNTAX; для Fanuc/Haas-колонок IF/WHILE → line-Raw (v0.16), для linuxcnc — файл-killer | **[REAL-SEM]** — L2-locked, тир B |
| 21 | bare-T (GRBL) | 2 | E_INVALID_INPUT | vince_BewareOfDog_v10_2_T2/_v10_3_T4 | — | — | **зонд у D, ждём** |
| 22 | end-not-last / G92-mid | 3 | смеш. | mit_hilbert_test, vis_knight, grbljs_sample | — | — | **зонд у D, ждём** (tball-engrave-laser #14 — соседний класс, но корень иной: дубль, не «не-последний») |

**Сумма: 5 (D) + 17 (TRUE-ERR) + 4 (AMB) + 10 (тир A) + 38 (тир B) + 5 (тир C) + 2 (lathe-этап 10) = 81.**

---

## 2. Ответы на спец-вопросы брифа

- **bare-T (vince_×2)** — зона D-зондов, ждём (табл. #21). Попутно: vince_BewareOfDog_v10.nc (без T) — Ok, так что отказ триггерится именно T-словом.
- **«P без потребителя» после NURBS-фикса**: NURBS-ложные срабатывания сняты (11 файлов butterfly/leto/nurbs/trident — теперь Ok+Raw). Остались **ровно 2** файла — `lcnc_tests_…arc-feed_arc_maxvel_xy.ngc` и `…_arc_time.ngc`, где `P` стоит в кадре **G2** (не G5): реальный rs274 это тоже отвергает → **TRUE-ERR**, не кандидат. **Curva_×4** в этот класс больше не входят: их P потребляется G5.2/G5.3 (строки контрольных точек), а отказ теперь на строке `#1=3` — параметр присваивается, но **ни разу не читается** (dead assignment). Это самый дешёвый подслучай тира B: drop+notice «unused parameter assignment» или line-Raw → Ok+Raw без всякой параметрики.
- **«R word without consumer»**: в 81 ERR **ни одного** срабатывания нет (grep по сообщениям свипа пуст). Гэп §6.71 (Fanuc `G10 L1x P_ R_` → Err вместо Raw{TABLE}) в Tier B не представлен ни одним файлом — фикс остаётся в брифе eid-fanuc, но на инвентаризацию не влияет.
- **Lexical garbage `by`**: подтверждено TRUE-ERR — rs274 отвергает вложенные скобки («Nested comment found», зонд-выкладка аудита #17, COVERAGE §4 «диспозиция честна»). У gcb-файлов первый вложенный `(` уже в 4-й строке ASCII-арта; наша диагностика называет остаток (`by LunaX…`) — текст сообщения можно выровнять с rs274-причиной, вердикт не меняется.
- **ZeroLength**: в текущем свипе **0 срабатываний** — класс закрыт (§3 v0.6/v0.17: zero-move → drop+notice, full-circle → 360°-дуга). Все arc-mismatch из 81 разложены по классам #3/#4/#9/#13-строке выше.

## 3. Зонд-доказательства (что проверено, а не вычитано)

1. **`eid-cli verify` локализует каждый отказ** (строка:колонка) — по нему вскрыты корни: comp311:27 (не «битая дуга», а потеря хода `g41 g1 y4`), placa:31, ncm41:653, ncm87:176 / ncmhaas:174 (`G3 X100. Z-181. R.5`), full-circle:11 (`N55 X.75`), calib:624, hse1p:37, duas_morsas:12, units-файлы (строки `g21`).
2. **Зонды `target/probe_c_inv/`** (созданы этим отчётом):
   - `g41_motion_drop.ngc` vs `no_g41_control.ngc`: с `G41 D1 G1 X60.5 Y95.5` → chord>2R; без G41 (тот же ход) → OK. **Механизм «Raw-блок глотает движение» подтверждён.**
   - `lathe_g18.ngc`: `G18; G0 X101 Z-181.5; G3 X100 Z-181 R.5` → chord>2R в радиус-семантике; в диаметральной (токарь, X/2) хорда = 0.707 ≤ 2R=1.0 → валидна. **Корень двух lathe-отказов = diameter-mode (этап 10), не битый файл.**
   - `placa_mimic`: воспроизводит цепочку placa — отказ только при потерянном ходе G41-блока.
3. **Константы контроллеров (web-сверка 2026-11-22):** LinuxCNC `TOLERANCE_INCH=0.0005 / TOLERANCE_MM=0.005` (src/emc/rs274ngc/interp_internal.hh; проверка радиуса — arc_data_ijk/r в interp_convert.cc); GRBL: radius-check у IJK-дуг **отсутствует**, но offset-слова обязательны в каждом дуговом блоке — error 2 `STATUS_GCODE_ARC_OFFSETS_MISSING`. → vis_hse1p (δ=0.0011 < 0.005mm) прошли бы ОБА контроллера; full-circle-файл не прошёл бы ни один (см. Sources).

## 4. Кандидаты на v0.20 (ранжировано по числу файлов)

| Тир | Фикс | n | Файлы → судьба | Якорь |
|---|---|---|---|---|
| **B** | Выровнять LinuxCNC-диалект с Macro-B-политикой v0.16: `#`-строки, `[expr]`-слова, IF/WHILE/GOTO/O-sub → **line-level Raw** (Raw ≠ интерпретация — Q10 остаётся закрытым), статические строки парсятся дальше | 38 | 30 параметрика + 7 word-без-числа + 1 IF → Ok+Raw (same-dialect); кросс-диалект — E_RAW_UNTRANSLATABLE как обычно | §1.1 + COVERAGE §4 Macro-B-строки |
| B-min | (подслучай) Curva_×4: dead assignment `#1=3` (не читается) → drop+notice | 4 | Curva ×4 → Ok (+1 NURBS-Raw-строка уже есть) | §1.1-минимум |
| **A** | Chain-continuity: Raw-блок, содержащий XYZ-слова хода (G41/G43-строки, rotary-only G0), обязан продвигать позиционную цепочку (или эмиссить ход+Raw-аннотацию) | 5 | comp311, cncsim_g84_g43_placa, ncm_g41_d_2d5, ncm_g434_rtcp_5x (частично: RTCP-семантика всё равно этап 12), ncm_g43_g69_3d5x → Ok/Ok+Raw | SPEC §2 принцип 2 (обязанность связывания), §8 Raw-механика |
| A | Допуск радиуса по диалекту: grbl — без проверки (реальный GRBL) или warning; linuxcnc — 0.005mm/0.0005in | 1 | vis_hse1p → Ok (calib δ=1.4мм — [AMB], вне счётчика тира A до профиль-решения) | §6.1/§6-валидация, EQUIVALENCE-допуски |
| A | Лекс-толерантность: GOTO → токен → Raw{SYNTAX} (как IF/WHILE); grbl-лексер: байты `c <= ' '` выбрасывать до парсера | 2 | gce_full_industrial_lathe, mit_cws_fresa → Ok+Raw / Ok | COVERAGE §4 (обе выкладки уже зафиксированы) |
| A | Дублированный program end → truncate-at-first + notice | 1 | tball-engrave-laser → Ok (хвост = Raw/notice) | §3 (зеркало auto-PEND v0.16) |
| A | Не валидировать NODE-легальность (M6-без-T) в теле O-sub (Raw-зона) | 1 | gmoccapy_macros_change_g43 → Ok+Raw | §8 правило двух классов |
| **C** (требует SPEC-ревизии) | §2.5 convert-at-read вместо отказа; §2(v0.11) per-axis frame_preset (partial G92); per-axis unknown/Z-hold для частичного end в разрыве (b) | 5 | units ×3, mit_cws_torno, cncsim_m98_m99_duas_morsas | ревизии §2.5/§2 |
| вне v0.20-L1 | Diameter-mode токарных дуг | 2 | ncm_g87_g88_g121_turn1, ncm_haas_g76_g15_eob_turn — реализуемо только с lathe-семантикой | SPEC §1.2 (этап 10) |

**Итог по потенциалу:** переход Err → Ok/Ok+Raw в v0.20 без SPEC-ревизий = **48 из 81** (тир A 10 + тир B 38); ещё **5** — при ревизиях тира C; **2** — только на этапе 10 (lathe); **4 AMB** — после профиль-решений (calib, zapcon_g81, zapcon_g65_ij, ncdrill1 — последние два скорее останутся Err/AMB); **17 TRUE-ERR** — остаются Err (реальные контроллеры тоже откажут); **5** — зона D.

## 5. TRUE-ERR остаток (17)

| Подкласс | n | Файлы | Контроллер-подтверждение |
|---|---|---|---|
| Вложенные комментарии | 9 | gcb ×8, gce_g71_bareint_testg71 | rs274 «Nested comment found» (COVERAGE §4) |
| Линейный ход без F | 2 | mit_pycnc_circles, mit_pycnc_test_parser | GRBL error «Undefined feed rate» |
| Дуга без offset-слов в блоке | 1 | grbljs_full-circle-without-xyz | GRBL error 2 (ARC_OFFSETS_MISSING); диагностику переименовать в «arc offsets missing» |
| P в кадре G2 | 2 | arc_maxvel_xy, arc_time | rs274 «P word with no G4/G10/…» |
| G53 без G0/G1 | 1 | vultr_g152_npt_pipe_example | LinuxCNC documented error |
| RepRap E-слова в grbl-файлах | 2 | mit_ddp_eck3, mit_ddp_m_retract | GRBL: E — не слово (отказ; свип-корень partial-G92 — вторичен, fires раньше) |

## 6. Отметки для соседних зон

- **Агент D (target/probe/g92/)**: классы #21/#22 табл. — ждём зонд-вердиктов; если D зафиксирует «G92-mid = REAL-SEM/policy» для grbljs_sample, класс #14 (дубль-PEND) примыкает к той же end-политике.
- **SPEC-рирайтер v0.20**: тир B не требует переоткрытия Q10 (Raw-механизм §8 существует; выравнивание linuxcnc-колонки с Fanuc/Haas Macro-B-политикой — матричная правка COVERAGE §4 + §1.1-примечание); тир A#3 требует заметки в §2 (принцип 2) о связывании через Raw-блоки.
- Зонды-файлы этого отчёта: `target/probe_c_inv/` (6 мини-файлов) — можно удалить после приёмки.

## Sources (web-сверка констант, 2026-11-22)

- [LinuxCNC interp_internal.hh — TOLERANCE_INCH/MM](https://github.com/LinuxCNC/linuxcnc/blob/master/src/emc/rs274ngc/interp_internal.hh), проверка радиуса — [interp_convert.cc](https://github.com/LinuxCNC/linuxcnc/blob/master/src/emc/rs274ngc/interp_convert.cc)
- [LinuxCNC forum: «Radius to end of arc differs…»](https://forum.linuxcnc.org/38-general-linuxcnc-questions/36494-error-message-radius-to-end-of-arc-differs-etc-etc) (допуск без INI-настройки, правка констант при пересборке)
- [GRBL issue #421 — error 2 STATUS_GCODE_ARC_OFFSETS_MISSING](https://github.com/gnea/grbl/issues/421), [Grbl v1.1 Error Messages](https://support.easel.com/hc/en-us/articles/40531503499155-Grbl-v1-1-Error-Messages), [SainSmart GRBL Quick Reference](https://www.sainsmart.com/blogs/news/grbl-v1-1-quick-reference)
