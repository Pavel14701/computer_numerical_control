//! eid-cli — тонкий бинарь над eid_cli (Агент E). Вся логика — src/lib.rs.
//!
//! EXIT-МАТРИЦА (чек-лист; автотесты — tests/cli_matrix.rs, имена cli_*):
//!   usage → 2:  unknown dialect | missing flag | duplicate flag |
//!               unknown flag | unknown command | лишний позиционный аргумент
//!   ok → 0:     no args (help) | --help/-h/help | --version | translate OK |
//!               verify OK | profile dialect ≠ --from (note в stderr, не отказ)
//!   fail → 1:   bad profile TOML | missing input file | output == input |
//!               output dir missing | adapter pending (grbl-hal/fluidnc) |
//!               translate parse/emit отказ (диагностики [stage] в stderr) |
//!               verify parse/validate/mock-sim отказ
//!
//! Итерация 4–5: live-адаптеры (grbl-1.1/linuxcnc/eid-l1) через registry
//! (eid-integration::translate); verify = parse + validate + mock-sim;
//! диагностики с привязкой к этапу ([stage] code: message @ src).

use eid_cli::run;
use std::process::ExitCode;

fn main() -> ExitCode {
    run(std::env::args().skip(1).collect())
}
