//! fuzz_eid_core_parse — libfuzzer-таргет канонического формата EID/L1
//! (eid-core EidTextAdapter, #36): парсер MUST-формы (SPEC §3) на
//! произвольном тексте. Вход: сырой байтовый буфер (до ~30 строк, см.
//! MAX_BYTES); критерий: parse/emit не паникуют. Запуск — fuzz/README.md.
#![no_main]

use eid_core::adapter::{Adapter, EidTextAdapter};
use libfuzzer_sys::fuzz_target;

/// Граница буфера: ~30 строк по ~1 КиБ — гигантские входы лишь замедляют
/// мутации, интересующие ветки парсера достижимы на коротких программах.
const MAX_BYTES: usize = 30 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_BYTES {
        return;
    }
    let text = String::from_utf8_lossy(data);
    let adapter = EidTextAdapter::new();
    // v0.2 #22: notices — в owning-результате parse.
    if let Ok(out) = adapter.parse(&text, "fuzz.eid") {
        let program = out.program;
        // round-trip стабилизация: emit→parse каноничного AST не паникует
        // (эмиссия может отказать по Unsupported* — это Ok-ветка, не crash).
        if let Ok(repr) = adapter.emit(&program) {
            let _ = adapter.parse(&repr.text, "fuzz_rt.eid");
        }
    }
});
