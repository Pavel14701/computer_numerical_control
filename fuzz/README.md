# fuzz — cargo-fuzz таргеты EID (#36)

libFuzzer-таргеты парсеров диалектов. Отдельный крейт **вне корневого
workspace** (nightly-only: libfuzzer-sys/санитайзеры не входят в stable
сборку репозитория; `[workspace]` в `Cargo.toml` изолирует fuzz от
корневого workspace, корневой `Cargo.toml` не модифицируется).

## Таргеты

| бин                     | крейт        | вход                          |
|-------------------------|--------------|-------------------------------|
| `fuzz_grbl_parse`       | `eid-grbl`   | байтовый буфер ≤ 30 КиБ (~30 строк G-кода), lossy-UTF8 |
| `fuzz_linuxcnc_parse`   | `eid-linuxcnc` | тот же                      |
| `fuzz_eid_core_parse`   | `eid-core` (EidTextAdapter) | тот же + round-trip emit→parse на Ok |

Критерий: `parse`/`emit` не паникуют — любой отказ выражается
`Vec<Diagnostic>` (SPEC §8). Найденный crash пишется libfuzzer в
`artifacts/` (gitignored) с минимальным воспроизводителем.

## Запуск

Требуется nightly и `cargo-fuzz` (однократно):

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
```

Сборка и прогон (из каталога `fuzz/` либо с `--manifest-path`):

```sh
cd fuzz
cargo +nightly fuzz build
cargo +nightly fuzz run fuzz_grbl_parse        # пустой корпус → мутирует с нуля
cargo +nightly fuzz run fuzz_linuxcnc_parse corpus/   # с сид-корпусом
cargo +nightly fuzz run fuzz_eid_core_parse -- -max_total_time=300
```

Полезные флаги libfuzzer: `-max_len=30720` (граница буфера таргета),
`-runs=N`, `-jobs=N -workers=N` (параллельные воркеры),
`artifacts/` — краши; `coverage/` — отчёты `cargo fuzz coverage`.

## Замечания

- Windows/Linux/macOS: cargo-fuzz поддерживает все три (на Windows
  MSVC-тулчейн; `-Zsanitizer` требует nightly).
- CI отдельных задач для fuzz сейчас нет (осознанно, итерация 7);
  рекомендации по регулярному прогону — при стабилизации корпуса.
- Не добавляйте `fuzz` в корневой `workspace.members`.
