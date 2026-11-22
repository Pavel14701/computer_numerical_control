# Tier B — инвентаризация L2-мишеней (агент B, 2026-11-23)

**Статус:** research-отчёт для DoD фазы 12.1 (вход дизайн-дока `EID-L2-DESIGN.md` §5).
Зона: `corpus/tier_b/{inbox,local}` — все **157** файлов (70 inbox + 87 local) прогнаны
автоматическим сканером маркеров + сквозной `eid-cli verify`-свейп по manifest-диалектам.
Крейты/документы/manifest не изменялись. Не коммитить (по заданию).

## 1. Метод

1. **Сканер маркеров** (Python, per-line): перед матчингом вырезаются
   paren-комментарии `( ... )` (известный паттерн ложных `#` внутри `( )`) и
   `;`-комментарии (per-file эвристика `;`=EOB vs `;`=comment; EOB-файлы
   `cncsim_*`/`ncm_haas_*` ставят `;` в конце строки — содержимое в корпусе не
   теряется, проверено вручную). Маркеры: `#N`/`#<name>`/`#N=`; `[ ]`-выражения
   (кроме grbl-строк `[MSG:...]`); `IF/WHILE/GOTO/THEN/DO/END`
   (case-insensitive, word-границы, включая Fanuc-формы `DO1`/`END1`);
   O-word-контроль (`O123 sub/while/if/call…` **и** именованные
   `O<name> sub/call/endsub` — второй паттерн нашёл 2 файла, пропущенных
   числовым O-regex: `lcnc_nc_files_g20sub.ngc`, `lcnc_m70_m72_demo.ngc`);
   M98/M99/G65/G66/M97; G68/G51/G51.1/G68.2; A/B/C-оси; G31/G38.x; G43.4;
   NURBS G5.2/G5.3/G2.2.
2. **Разбор ложных срабатываний вручную**: в Macro-B буквы аргументов
   `G65 P9725 A1.` — НЕ ось A (исключено из ABC-ценза rensh/coyo);
   `#[5202+[20*#5220]]` — вычисляемая ссылка на системную переменную
   (Assign+expr, не XYZ); `(debug, …)` в LinuxCNC — исполняемый statement с
   параметрической интерполяцией, НЕ комментарий (19 файлов — см. §6.9).
3. **Сквозной L1-свейп**: `eid-cli verify --dialect <manifest>` по всем 157 —
   фактический статус парсера L1 на момент замера (prebuilt
   `target/debug/eid-cli.exe`, не пересобирался).

## 2. Главный вывод: реальное число L2-мишеней vs заявка «26»

| Категория | Файлов | Комментарий |
|---|---|---|
| **§2.4-класс (нужны L2Stmt)** | **53** | fanuc-MB 7 + haas-MB 12 + linuxcnc 34 |
| Только ABC-оси (без stmt-конструкций) | 6 | vis_SmilinJack (A), cncsim_g76_eob, ncm_g41_d, ncm_g43_g69, ncm_g87, ncm_haas (C) |
| **Итого по маркерам задачи** | **59** | 7 inbox + 52 local |
| NURBS G5.2/G5.3/G2.2 (вне §2.4, отдельная категория) | 15 | все local/linuxcnc (§4) |
| Только спец-циклы вне маркеров L2 (не L2-цели 12.1) | 3 | gce_g74_peck_bareint (G74), gce_g76_thread_fanuc (G76), lcnc_g331_rigid_tap_docs (G33.1) |
| Файлов без L2-конструкций | 80 | остальное (157 − 59 − 15 − 3) |

**Заявку «26 отложенных файлов» подтвердить нельзя**: минимальная честная
оценка — **53 файла §2.4-класса** (вдвое больше), широкая — 59 (+15 NURBS =
74 L2-релевантных). Ни один проверенный кандидат на происхождение «26» не
совпадает: SPEC Q10 говорит «25+» (без точного числа); fanuc+haas в manifest =
30; parse-rejected на L1 сегодня = 50. Рекомендация: в `EID-L2-DESIGN.md` §5
заменить «26» на **53 (§2.4-класс)** с этой таблицей как источником; NURBS ×15 —
отдельной строкой (§4); спец-циклы ×3 — вне скоупа 12.1.

## 3. Пофайловая таблица §2.4-класса + ABC-only

L2Stmt-колонка выведена механически из ценза; «вне-§2.4» — пробелы для
дизайн-ревизии. 55 строк = 53 stmt-файла + 2 ABC-only с пустой stmt-колонкой
(vis_SmilinJack, cncsim_g76_eob); ещё 4 ABC-only файла (ncm_g41_d,
ncm_g43_g69, ncm_g87, ncm_haas) в таблицу не включены — см. §5.
Диалект-предположение подтверждено PROVENANCE-источниками (cnc4me=fanuc-MB,
Renishaw-NGC/ZapCon/zodiac/vulture=haas, LinuxCNC-repo=linuxcnc) и кодом
(G154/G103/M97=haas; G154 P98 вне p-домена; `;`-EOB).

| Файл | Диалект | L2Stmt (§2.4) | Вне-§2.4 пробелы |
|---|---|---|---|
| `inbox/gce_full_industrial_lathe.g` | fanuc-MB | Assign | G31-пробинг; B/A/C-оси (B-усилие, live-tooling); vendor M53/M59/M65-M79/M86-M89/M115; `,C`/`,R` comma-words; G4 U; G95; G41; sys-var #5065/#1133 |
| `inbox/mpe_g65_macro_centerline_probe.nc` | fanuc-MB | Assign, IfGoto, CallSub | G31-семантика в вызываемых 9810-макросах (вне корпуса); #0-нулл-тест; ABS; sys-var #3000-alarms/#148 |
| `inbox/mpe_g65_macro_deep_drill.nc` | fanuc-MB | Assign, IfGoto, CallSub | sys-var #5003/#3000-alarms; #0-нулл-тест |
| `inbox/mpe_g65_macro_material_verify.nc` | fanuc-MB | Assign, IfGoto, CallSub | ABS; #0-нулл-тест; sys-var #3000/#142-148 |
| `inbox/mpe_toolpath_profiling_m1.nc` | haas-MB | Assign | косвенная адресация #[#540]; sys-var #3001 (таймер); деление |
| `inbox/mpe_while_clear_variables.nc` | fanuc-MB | Assign, While | косвенная адресация #[#1]; #0-нулл |
| `inbox/vis_SmilinJack_-_ThreeAxisFlatMachine_Relief.tap` | fanuc-MB | — | A-ось indexer (кинематика, не stmt) |
| `local/cncsim_g154_g43_haas_vf2.nc` | haas-MB | CallSub | M97 P\<block\> (local sub по N-номеру); G154 P1-P2 (Haas extended WCS) |
| `local/cncsim_g76_eob_ciclos.nc` | fanuc-MB | — | C-ось `,C` comma-spindle-sync; G71/G76 lathe-циклы; G70; M27; `;`-EOB |
| `local/cncsim_m98_m99_duas_morsas.nc` | fanuc-MB | CallSub | — |
| `local/coyo_tram_b_probe_o09817.nc` | haas-MB | Assign, CallSub | B-ось (B#10189 — параметр-слово с переменной); Renishaw G65 P9832/9810/9817-цепочка; #10189 vendor-var |
| `local/lcnc_configs_sim_gmoccapy_macros_change.ngc` | linuxcnc | Assign, IfGoto, CallSub | именованные sys-чтения #\<_task\>/#\<_hal[…]>/#\<_ini[…]>; G38.2-пробинг; remap-контекст #\<tool\>/#\<pocket\>; G53; return [expr] |
| `local/lcnc_configs_sim_gmoccapy_macros_change_g43.ngc` | linuxcnc | Assign, IfGoto, CallSub | endsub [retval]; G43 |
| `local/lcnc_configs_sim_gmoccapy_macros_go_to_position.ngc` | linuxcnc | Assign, CallSub | параметр-слова `X #1` (пробелы); debug-интерполяция |
| `local/lcnc_g33_threading.ngc` | linuxcnc | Assign, While, CallSub | G33-синхронное нарезание; sqrt[] |
| `local/lcnc_m61_output_demo.ngc` | linuxcnc | Assign, IfGoto, CallSub | именованные O\<name\>-subs; endsub [#1]; return [-1]; sys-var #5399; M61 digital-out |
| `local/lcnc_m70_m72_demo.ngc` | linuxcnc | CallSub | M70/M71/M72 (сохранение/восстановление модального состояния); debug с именованными sys-чтениями |
| `local/lcnc_m73_demo.ngc` | linuxcnc | Assign, IfGoto, CallSub | M73 (autoremember); call [args]; именованные sys; DEBUG |
| `local/lcnc_nc_files_3D_Chips.ngc` | linuxcnc | Assign | выражения в motion-словах (4684 скобок-хитов); named assigns |
| `local/lcnc_nc_files_cone.ngc` | linuxcnc | Assign, While, CallSub | C-ось `c#100` (rotary + переменная); g43.1 динамический TLO; cos/sin |
| `local/lcnc_nc_files_daisy.ngc` | linuxcnc | Assign, IfGoto, CallSub | арифметика [440\*1.78]; else-ветки |
| `local/lcnc_nc_files_factorial.ngc` | linuxcnc | Assign, IfGoto, CallSub | РЕКУРСИЯ call [[#1]-1]; именованный local #\<_value\>; return [expr] |
| `local/lcnc_nc_files_flowsnake.ngc` | linuxcnc | Assign, IfGoto, CallSub | рекурсия по level; named params ×55 |
| `local/lcnc_nc_files_g20sub.ngc` | linuxcnc | CallSub | M70/M71 внутри подпрограммы |
| `local/lcnc_nc_files_g76.ngc` | linuxcnc | CallSub | G76 lathe thread-цикл (LinuxCNC-форма) |
| `local/lcnc_nc_files_g881.ngc` | linuxcnc | Assign, IfGoto, CallSub | EXISTS[]; именованные параметры sub; debug; M75-remap-контекст |
| `local/lcnc_nc_files_g881min.ngc` | linuxcnc | CallSub | endsub [retval]; #\<n_args\> remap-параметр |
| `local/lcnc_nc_files_g882.ngc` | linuxcnc | CallSub | endsub [retval]; M75 |
| `local/lcnc_nc_files_gridprobe.ngc` | linuxcnc | Assign, IfGoto, While, CallSub | fix[]; вложенные while |
| `local/lcnc_nc_files_increment.ngc` | linuxcnc | Assign, CallSub | — |
| `local/lcnc_nc_files_ngcgui_lib_arc1.ngc` | linuxcnc | Assign, IfGoto, While, CallSub | trig cos/sin; AND/OR; named ×430; 6+ позиционных аргументов |
| `local/lcnc_nc_files_ngcgui_lib_arc2.ngc` | linuxcnc | Assign, IfGoto, CallSub | ATAN[y]/[x] двухаргументный |
| `local/lcnc_nc_files_ngcgui_lib_backlash.ngc` | linuxcnc | Assign, IfGoto, While, CallSub | ABS; AND/OR |
| `local/lcnc_nc_files_ngcgui_lib_db25.ngc` | linuxcnc | Assign, IfGoto, CallSub | call с 4 аргументами [a][b][c][d]; tan |
| `local/lcnc_nc_files_ngcgui_lib_gosper.ngc` | linuxcnc | Assign, IfGoto, CallSub | рекурсия; ATAN/COS |
| `local/lcnc_nc_files_ngcgui_lib_lathe_g76base.ngc` | linuxcnc | Assign, IfGoto, CallSub | G76-цикл; AND-цепочки |
| `local/lcnc_nc_files_ngcgui_lib_lathe_g76diam.ngc` | linuxcnc | Assign, IfGoto, CallSub | вызов g76base |
| `local/lcnc_nc_files_probe_basic_probe_macros_probe_back_edge_angle.ngc` | linuxcnc | Assign, IfGoto, CallSub | вычисляемые sys-ссылки #[5202+[20\*#5220]]; AND; sys-var #5400/#5220/#5063 |
| `local/lcnc_nc_files_probe_basic_probe_macros_probe_back_left_inside.ngc` | linuxcnc | Assign, IfGoto, CallSub | то же + G38-семантика через вызовы |
| `local/lcnc_nc_files_probe_basic_probe_macros_probe_back_left_outside.ngc` | linuxcnc | Assign, IfGoto, CallSub | то же |
| `local/lcnc_nc_files_remap-subroutines_change.ngc` | linuxcnc | Assign, IfGoto, CallSub | remap-контекст #\<pocket\>; return [-1] |
| `local/lcnc_nc_files_remap-subroutines_on_abort.ngc` | linuxcnc | Assign, IfGoto, CallSub | elseif-цепочка |
| `local/lcnc_polar_at_angle.ngc` | linuxcnc | Assign | полярные координаты @radius^angle + ^[expr] (не Macro-B/не O-word!) |
| `local/lcnc_tests_interp_good_good-arc.big.imperial.center-format.ngc` | linuxcnc | Assign | выражение в arc-слове `I[…]` c sqrt |
| `local/lcnc_tests_interp_good_good-arc.big.metric.center-format.ngc` | linuxcnc | Assign | то же |
| `local/ncm_g434_rtcp_5x_milling.nc` | fanuc-MB | RtcpMode | RTCP G43.4; A/C-оси ×83844 (кинематика) |
| `local/rensh_g04p_bare_xyz_o09811.nc` | haas-MB | Assign, IfGoto, While, CallSub | G31-пробинг; G103 P1; WHILE+DO1/END1; косвенная адресация #[#161+2]; пробелы в word-value (сейчас strict-reject); bare-integer DPI; sys-var #5041-43/#3001/#3000 |
| `local/rensh_g154p98_tbd_o09006.nc` | haas-MB | Assign, IfGoto, CallSub | IF..THEN-присваивание; G154 P98 (вне p-домена); G31; A-ось; M98+M99; G43 H#3026 (var в H-слове); G103 |
| `local/rensh_g65_probe_bore_o09351.nc` | haas-MB | Assign, IfGoto, CallSub | G31; G103; косвенная адресация #[#161]; пробелы word-value; sys-var #3026 |
| `local/rensh_settings_check_g103p1_o09725.nc` | haas-MB | Assign, IfGoto, CallSub | IF..THEN; Haas sys-var #20045+/#598/#179/#561; G103 |
| `local/vultr_m97_l_macro_o9016.nc` | haas-MB | Assign, IfGoto, CallSub | bare-IF-assign (IF [cond] stmt без THEN); M97 P301 L[expr] — expr в L-слове; ABS; sys-var #3003 |
| `local/zapcon_g31_tsc_o9613.nc` | haas-MB | Assign, IfGoto, CallSub | G31 ×2; G103; bare-IF-assign; M98+M99; G43 H#3026; sys-var #3004 |
| `local/zapcon_g65_ij_g154p11_endmill_ex.nc` | haas-MB | CallSub | G154 P11 (вне p-домена); G65 H/I/J-аргументы |
| `local/zapcon_g81_p0_g154p10_drill_ex.nc` | haas-MB | CallSub | G154 P10; G65 |
| `local/zodiac_toolbreak_m59_g154pvar.nc` | haas-MB | Assign, CallSub | G154 P#5 — ПЕРЕМЕННАЯ в P-слове WCS; M59 digital-out; G43 H#3026; bare-числа |

Суммарно: **§2.4-класс 53** (fanuc-MB 7, haas-MB 12, linuxcnc 34).
Потребность в L2Stmt: Assign ×45, CallSub ×45, IfGoto ×30, While ×7,
RtcpMode ×1, **WithTransform ×0**.

## 4. NURBS — отдельная категория (15 файлов, все local, linuxcnc)

`lcnc_nc_files_nurbs_G5_*`: G5.2/G5.3-параметрика NURBS ×14
(butterfly ×3, nurbs ×3, trident ×4, leto, Curva_Chitarra, Curva_Delfino,
Curva_Mazatrol) + **G2.2**-сплайн ×1 (`Curva_Stella` — не G5.x; отдельный
spline-код, зафиксировать в дизайн-доке). В 4 Curva-файлах — мёртвое
`#1=<n>` (присваивание без единого чтения). COVERAGE v0.10: NURBS =
line-level Raw. Это L2-цели motion-узла (не §2.4-stmt): если 12.1 их берёт —
нужен отдельный NURBSStmt/узел; иначе скоуп 12.1 = 53.

## 5. ABC-оси, RTCP и трансформации — ценз

- **A/B/C-оси: 10 файлов.** A: vis_SmilinJack; B: coyo_tram_b_probe;
  B/A/C: gce_full_industrial_lathe; A/C: ncm_g41_d, ncm_g43_g69, ncm_g434;
  C: cncsim_g76_eob, ncm_g87_g88_g121_turn1, ncm_haas_g76_g15_eob_turn,
  lcnc_nc_files_cone (`c#100` — rotary-слово с переменной). Для 12.1
  достаточно токенизации осевых слов; кинематика — L3-трек.
- **G43.4 (RTCP): 1 файл** — `ncm_g434_rtcp_5x_milling.nc` (единственный
  публичный сэмпл; 2.1MB; ×83 844 A/C-слов). RtcpMode подтверждён ровно
  одной мишенью.
- **G68/G51/G51.1/G68.2: 0 файлов во всём корпусе.** WithTransform из §2.4
  НЕ имеет ни одной мишени. Единственный трансформационный код в корпусе —
  **G69** (cancel) в шапках 3 ncm-файлов, без предшествующего G68.
  Для дизайн-дока: WithTransform оставить как резервируемый L2Stmt
  (санити-вектор писать руками) или вынести за 12.1 — corpus-DoD его не
  покроет.

## 6. Агрегированные вне-§2.4 пробелы (для дизайн-ревизии)

1. **Пробинг**: G31 ×5 (gce_full, rensh_g04p_bare_xyz, rensh_g154p98_tbd,
   rensh_g65_probe_bore, zapcon_g31_tsc), G38.2 ×1 (gmoccapy_change) — 6 файлов.
2. **Системные переменные** (rvalue): Fanuc #3000/#3001/#3003/#3004,
   #5003/#5041-43/#5063/#5065; Haas-расширения #20045+, #3026, #598, #10189
   (coyo); LinuxCNC #\<_task\>, #\<_hal[…]>, #\<_ini[…]>, #5399, #5220, #5201+ —
   включая вычисляемые ссылки `#[expr]`.
3. **Функции выражений**: ABS, SQRT, SIN/COS/TAN, ATAN[y]/[x] (2-арг),
   EXISTS[] (g881), fix[] (gridprobe); операторы AND/OR/EQ/NE/GT/GE/LT/LE.
4. **Структурный O-word-if LinuxCNC**: `O..if/elseif/else/endif` — блоки, не
   fanuc-IF-GOTO (IfGoto-колонка включает, но семантика шире: elseif-цепочки —
   remap_on_abort).
5. **Рекурсия**: factorial, flowsnake, gosper — `call` из тела sub; нужны
   стек кадров + лимит глубины.
6. **Возвраты значений**: `endsub [expr]`, `return [expr]`.
7. **Косвенная адресация**: `#[#540]`, `#[#161+2]`, `#[5202+[20*#5220]]`.
8. **Haas-специфика**: bare-IF-assign без THEN (`IF [cond] stmt` — vultr_m97,
   zapcon_g31); M97 P\<block\> L\<expr\> (local call по N-номеру);
   G154 P#5 (переменная в P-слове WCS — zodiac); G103 P1 (block-delete) ×6
   файлов; `;`-EOB; пробелы letter-value (`X [ #10 …` — сейчас strict-reject
   L1); G65-аргументы H/I/J/Q/R; bare-integer DPI.
9. **LinuxCNC debug/print-стейтменты**: `(debug, …)`/`(print, …)` — 19 файлов —
   исполняемые statements с интерполяцией параметров; лексер
   «paren-комментарий» неверен для них на L2 (влияет на подсчёт `#`-хитов,
   классификацию файлов не меняет).
10. **Полярные координаты LinuxCNC**: `@radius^angle` + `^[@expr]`
    (lcnc_polar_at_angle) — отдельный синтаксис, ни Macro-B, ни O-word.
11. **Vendor M-коды в L2-контексте**: M53/M59/M65-M79/M86-M89/M115
    (gce_full, turn-mill B-ось), M59/M61/M75/M97/M70-M73 — семантика вне
    stmt-парсера (Raw/ABSORB-диспозиции отдельно).

## 7. L1-свейп: текущий статус 59 L2-файлов у eid-cli

- **parse-ok: 40** — макро-текст проходит как Raw/R1-text (same-dialect
  translate возвращает тело макроса эхом): L1 «проводит», но не
  интерпретирует — это и есть отложенные на 12.1.
- **parse-reject: 19 — причины почти все НЕ L2**: Haas-строгость
  «space between word letter and value» ×5 (rensh ×3, vultr, zapcon_g31);
  comma-слова `,C`/`,R` lexical ×4 (gce_full, cncsim ×3); геометрия дуг
  (chord/ρ mismatch) ×4 (ncm ×4); `M6 без T-слова` ×2 (gmoccapy remap);
  E_RUNTIME_STATE ×4 (vis A-axis unknown, zapcon ×2 machine-frame/cycle-first);
  BadNumber ×1 (zodiac:27). Следствие для 12.1: **фиксировать DoD по
  конструкциям (этот ценз), не по текущему ok/reject L1** — иначе
  Haas-strict и arc-валидация замаскируют прогресс.

## 8. DoD-чек-лист 12.1 по диалектам

**fanuc-Macro B парсер — 7 файлов** (gce_full_industrial_lathe,
mpe_g65_macro_centerline_probe, mpe_g65_macro_deep_drill,
mpe_g65_macro_material_verify, mpe_while_clear_variables,
cncsim_m98_m99_duas_morsas, ncm_g434_rtcp_5x):
Assign ×5 (вкл. `#[expr]`-косвенность), IfGoto ×3 (`IF[..]GOTO n`,
`#0`-нулл-тест), While ×1 (`WHILE..DO1..END1`), CallSub ×4 (G65 P\<prog\>
+ таблица аргументов A–Z→#1–#33, M98/M99), RtcpMode ×1 (ncm_g434 — можно
вынести в RTCP-трек); sys-var rvalue #3000/#5003; alarm `#3000=n` →
диагностика.

**haas-Macro B парсер — 12 файлов** (mpe_toolpath_profiling_m1,
cncsim_g154_g43_haas_vf2, coyo_tram_b_probe, rensh ×4, vultr_m97_l_macro,
zapcon ×3, zodiac_toolbreak_m59_g154pvar):
всё fanuc-MB-ядро, плюс IF..THEN-assign ×2, bare-IF-assign ×2,
M97-local-call ×2 (с L-expr в vultr), косвенная адресация, Haas
sys-var-нумерация (#20045+, #3026, #598), G154 P#-переменная,
G65-аргументы H/I/J; лексер-предусловия: G103, tolerant-пробелы в
word-value, `;`-EOB.

**linuxcnc O-word+named парсер — 34 файла**:
Assign ×31 (named `#<name>` и числовые), IfGoto ×21 (формат
`O..if/elseif/else/endif` — структурированный), While ×5
(`O..while/endwhile`), CallSub ×30 (`O<num|<name>> sub/call/endsub`,
`return [expr]`, `endsub [expr]`, позиционные аргументы `call [a][b]…`,
рекурсия ×3); выражения в motion-словах (`X[#<zscale>*10.]`, `i[300+…]`),
функции SQRT/ABS/ATAN/SIN/COS/TAN/EXISTS/fix, AND/OR;
debug/print-стейтменты; решить судьбу полярного `@`/`^` (1 файл) —
парсить или оставлять Raw.

**Явно вне 12.1** (отдельные треки): ABC/RTCP-кинематика (10+1 файл),
NURBS ×15, пробинг-семантика G31/G38.x (6), трансформации G68/G51
(0 мишеней; только G69-cancel ×3), спец-циклы (G33/G33.1/G71/G74/G76/G12.1)
и vendor-M-семантика.

---
*Метод-артефакты (вне репо): `C:\tmp\l2scan.py` (сканер маркеров),
`C:\tmp\l2sweep.py` (L1-свейп), `C:\tmp\l2scan_results.json`,
`C:\tmp\l2sweep_results.json`, `C:\tmp\l2table.txt` (сырые данные).
Коммит не выполнялся.*
