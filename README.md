# eid — G-code translator через канонический промежуточный диалект

Трансляция G-кода между диалектами станков — GRBL 1.1, LinuxCNC, Fanuc, Haas —
через каноническое промежуточное представление **EID/L1** с формально
определённой эквивалентностью (motion-stream, resolved-state). Pending:
grblHAL, FluidNC (COVERAGE-колонки готовы, адаптеров нет).

**Статус:** MVP-цикл закрыт (итерации 0–7, 12/12 критериев приёмки —
`REPORT.md`); очередь — разморозка API v0.2 (`INTERFACE-V0.2-DESIGN.md`).
Workspace: **982 теста / 0 упавших** (`cargo test --workspace`).

## Диалекты

| Диалект | Статус | parse → EID | EID → emit | Legality | Тесты | Что уже покрыто до адаптера |
|---|---|---|---|---|---|---|
| `grbl-1.1` | live | ✔ | ✔ | таблица (legal 10 / illegal 9) | 122 | полный словарь COVERAGE; колонки grblHAL/FluidNC наследуют GRBL 1.1 (`=`) |
| `linuxcnc` | live | ✔ | ✔ | таблица (legal 11 / illegal 8) | 150 | полный словарь COVERAGE; канонический шаблон циклов §7.7 |
| `fanuc` | live | ✔ | ✔ | таблица | 217 | COVERAGE-колонка + Macro B → line-level Raw; DPI; EOB `;`; 17 корпусных файлов обрабатываются live (ok=6 / diag=11, crash=0) |
| `haas` | live | ✔ | ✔ | **LegalityCondition-enum** — первый адаптер на стандарте v0.2 | 176 | COVERAGE-колонка + WCS-слоты p=7..9 → G110–G112; P-dot-rule; M-коллизии (M07/M29/M13) |
| `eid-l1` | live | ✔ | ✔ | — | — | канонический формат: `grammar.ebnf` v0.4 (magic провода «EID/L1 v0.3», back-compatible superset) |
| `grblHAL` | pending | — | — | — | — | COVERAGE-колонка есть: наследование GRBL 1.1, source-чеки core gcode.c закрыты (§6.12), остатки — `?`-реестр |
| `FluidNC` | pending | — | — | — | — | COVERAGE-колонка есть: словарь наследует GRBL 1.1, дельты `?` до сверки с wiki |

Pending-адаптеры не молчат: translate на них — `AdapterPending` (exit 1 с
сообщением).

**Циклы:** развёртка canned cycles — единый канонический шаблон SPEC §7.7 на
общих константах v0.18, зафиксированный на четырёх поверхностях (GRBL 1.1 —
Raw-pinning «canned cycles вне словаря»; LinuxCNC/Fanuc/Haas — развёртка);
перекрёстные проверки `semantic_diff = 0`. Референс-оракул — реальный rs274
(LinuxCNC 2.9, Docker, headless): все 48 позитивных векторов e2e —
**0 divergences** (motion stream).

## CLI

```
# translate: GRBL → Fanuc. --profile опционален: без него берётся
# встроенный дефолт по цели (fanuc_default)
eid translate --from grbl --to fanuc --input part.ngc --output part_fanuc.nc

# verify: parse + validate + mock-sim; --dialect вместо полного профиля
eid verify --dialect linuxcnc --input part.ngc
```

Диалекты: `grbl-1.1` (алиас `grbl`), `linuxcnc`, `fanuc`, `haas`, `eid-l1`.
Диагностики печатаются с этапом возникновения (`[parse] E_INVALID_INPUT: …`
@ строка:колонка). Exit-коды: 0 ok / 1 отказ (диагностики трансляции,
pending-адаптер, лимиты профиля) / 2 usage.

## Архитектура

```
 GRBL 1.1 ─┐  parse → canonicalize → verify          emit + legality  ┌─→ GRBL 1.1
 LinuxCNC ─┤  (modal state → resolved-state AST;                      ├─→ LinuxCNC
 Fanuc    ─┤   RAW двух классов: same-dialect verbatim,        EID/L1 ┼─→ Fanuc
 Haas     ─┤   кросс-диалект — E_RAW_UNTRANSLATABLE)     детерминир. ├─→ Haas
 EID/L1   ─┘                                                        └─→ EID/L1
                      ▲
     rs274 (LinuxCNC 2.9, Docker) — референс-оракул эквивалентности
```

Пять машин трансляции вокруг одного канона. Каждый диалект — пара parse↔emit;
emit-путь guarded legality-таблицей (ConversionTarget-паттерн: legal/illegal +
коды как данные — эмиттер отказывает, а не выдумывает код). Каноническое ядро —
`eid-core`: AST, verify-pass (каждая диагностика помечена этапом:
Parse/Canonicalize/Verify/Emit), текстовый формат.

## Trust story

**Ноль внешних зависимостей на пути, который генерирует твой G-код.**
Все пять машин трансляции — `eid-core`, `eid-grbl`, `eid-linuxcnc`,
`eid-fanuc`, `eid-haas` — std-only: единственные зависимости — path-deps на
`eid-core` (у него ноль зависимостей вовсе); проверяется `Cargo.toml` (см.
также §6 плана). Вне трансляционного пути — два сервисных контура, оба
serde+toml: профили/реестр (`eid-integration`: разбор TOML-профилей, дефолты
встроены в бинарь `include_str!`) и QA-инфраструктура (`eid-harness`:
corpus manifest, генератор Tier A v1, мутационный фаззер; у адаптеров —
только dev-dependencies). Это конфигурация и тестовый контур — код,
преобразующий G-код, внешних crate не тянет. serde_yaml удалён
(R-1 закрыт, PLAN v0.1.3).

Формат — версионируется (`grammar.ebnf`; magic-строка каждого EID-файла =
версия формата). Эквивалентность формально определена (`EQUIVALENCE.md`):
что сравнивается, что не входит, все допуски — с фиксированными
коэффициентами, меняющимися только ревизией.

## Структура

```
crates/
  eid-core/         AST EID/L1, диагностика, verify-pass, EID parser/emitter  (std-only)
  eid-grbl/         GRBL 1.1: parse ↔ emit                                    (std-only)
  eid-linuxcnc/     LinuxCNC: parse ↔ emit, канонический шаблон циклов        (std-only)
  eid-fanuc/        Fanuc: parse ↔ emit, Macro B → Raw, DPI, EOB `;`          (std-only)
  eid-haas/         Haas: parse ↔ emit, WCS p=7..9 → G110–G112, P-dot-rule    (std-only)
  eid-harness/      semantic_diff, corpus manifest, генератор/мутаторы/фаззер
  eid-integration/  реестр адаптеров, machine profiles (TOML), sim (mock + rs274 real)
  eid-cli/          translate / verify
corpus/             Tier A (68 векторов: 48 позитивных с golden + 20 негативов)
                    + Tier B (144 файла) + manifest (см. CORPUS-CONTRACT.md)
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
| `SPEC-EID-L1.md` (v0.18) | Спека промежуточного диалекта: модель, 10 опов, инварианты, §7.7-шаблон циклов |
| `COVERAGE.md` (v0.9.3) | Матрица диспозиций G/M-кодов 6 диалектов (GRBL 1.1, grblHAL, FluidNC, LinuxCNC, Fanuc, Haas) + реестр `?` (72 поз.) |
| `EQUIVALENCE.md` (v0.3.1) | Контракт эквивалентности: фреймы, допуски, исключения |
| `CORPUS-CONTRACT.md` (v0.7.1) | Корпус: тиры Tier A, Tier B, протокол классификации красного |
| `CONSTRUCT-REGISTRY.md` (v0.0) | Навигационный срез «конструкция × диалект» по COVERAGE + корпусное закрытие |
| `INTERFACE-V0.2-DESIGN.md` (v0.1) | План разморозки API v0.2 (5 волн, решения на измерениях аллокаций) |
| `grammar.ebnf` (v0.4) | Авторитетный текстовый формат EID (D1–D10) |
| `IMPLEMENTATION-PLAN.md` (v0.1.3) | 5 агентов + оркестратор, итерации, приёмка |

## Сборка

```
cargo test --workspace
```

CI: fmt + clippy gates. rs274-референс — опциональный Docker-контур
(`docker/linuxcnc-sim.Dockerfile`); без Docker соответствующие тесты SKIP
(не красные).
