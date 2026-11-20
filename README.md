# eid — G-code translator через канонический промежуточный диалект

Трансляция G-кода между диалектами станков (GRBL 1.1 ↔ LinuxCNC; далее — семейства
grblHAL/FluidNC/Fanuc) через каноническое промежуточное представление **EID/L1** с
формально определённой эквивалентностью (motion-stream, resolved-state).

**Статус:** MVP в разработке (итерации 0–3 из 6; план — `IMPLEMENTATION-PLAN.md`).

## Trust story

**Ноль внешних зависимостей на пути, который генерирует твой G-код.**
Крейты компиляционного пути — `eid-core`, `eid-grbl`, `eid-linuxcnc` —
std-only: ни одного внешнего crate (проверяется `Cargo.toml`, см. также
§6 плана). Профили и тестовая инфраструктура (`eid-harness`, `eid-integration`)
используют `serde`/`serde_yaml` — это не влияет на трансляцию; риск-запись
и план миграции: `IMPLEMENTATION-PLAN.md` §6 (R-1).

Формат — версионируется (`grammar.ebnf`; magic-строка каждого EID-файла =
версия формата). Эквивалентность формально определена (`EQUIVALENCE.md`):
что сравнивается, что не входит, все допуски — с фиксированными
коэффициентами, меняющимися только ревизией.

## Структура

```
crates/
  eid-core/         AST EID/L1, диагностика, EID parser/emitter   (std-only)
  eid-grbl/         GRBL 1.1: parse ↔ emit                        (std-only)
  eid-linuxcnc/     LinuxCNC: parse ↔ emit                        (std-only)
  eid-harness/      semantic_diff, corpus manifest, test runner
  eid-integration/  machine profiles, (симуляция — итерация 4)
  eid-cli/          translate / verify
corpus/             векторы Tier A + manifest (см. CORPUS-CONTRACT.md)
docs → *.md         см. ниже
```

## Документы (версионированы, журнал — CHANGELOG.md)

| Документ | Что это |
|---|---|
| `SPEC-EID-L1.md` | Спека промежуточного диалекта: модель, 10 опов, инварианты |
| `COVERAGE.md` | Матрица диспозиций G/M-кодов 4 диалектов + реестр `?` |
| `EQUIVALENCE.md` | Контракт эквивалентности: фреймы, допуски, исключения |
| `CORPUS-CONTRACT.md` | Корпус: Tier A/B, протокол классификации красного |
| `grammar.ebnf` | Авторитетный текстовый формат EID (D1–D10) |
| `IMPLEMENTATION-PLAN.md` | 5 агентов + оркестратор, итерации, приёмка |

## Сборка

```
cargo test --workspace
```
