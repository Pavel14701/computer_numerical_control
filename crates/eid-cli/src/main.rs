//! eid-cli — тонкий бинарь над eid_cli (Агент E). Вся логика — src/lib.rs.
//!
//! EXIT-МАТРИЦА (чек-лист; автотесты — tests/cli_matrix.rs, имена cli_*):
//!   usage → 2:  unknown dialect | missing flag | duplicate flag |
//!               unknown flag | unknown command | лишний позиционный аргумент |
//!               --profile + --dialect вместе (verify)
//!   ok → 0:     no args (help) | --help/-h/help | --version | translate OK |
//!               verify OK | profile dialect ≠ --from (note в stderr, не отказ) |
//!               --profile не задан → дефолт-профиль по --to / --dialect / linuxcnc
//!   fail → 1:   bad profile TOML (в т.ч. пост-валидация семантики #26) |
//!               missing input file | output == input | output dir missing |
//!               adapter pending (grbl-hal/fluidnc) |
//!               translate parse/emit отказ (диагностики [stage] в stderr) |
//!               лимиты профиля превышены (E_LIMIT_EXCEEDED, #2) |
//!               verify parse/validate/mock-sim отказ
//!
//! Итерация 4–5: live-адаптеры (grbl-1.1/linuxcnc/eid-l1) через registry
//! (eid-integration::translate); verify = parse + validate + mock-sim;
//! диагностики с привязкой к этапу ([stage] code: message @ src).
//! Итерация 6 (аудит): --profile опционален (#10), профиль в конвейере (#2),
//! emit_preamble (#11), fanuc pending (#25 — закрыт, см. ниже).
//! Регистрация fanuc (Агент E): live-адаптер eid-fanuc в registry —
//! translate --from/--to fanuc обрабатывается реально; grbl-hal/fluidnc
//! остаются pending.

use eid_cli::run;
use std::process::ExitCode;

fn main() -> ExitCode {
    run(std::env::args().skip(1).collect())
}
