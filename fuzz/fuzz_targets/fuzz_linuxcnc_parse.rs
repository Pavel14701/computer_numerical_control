//! fuzz_linuxcnc_parse — libfuzzer-таргет парсера LinuxCNC (eid-linuxcnc, #36).
//! Вход: сырой байтовый буфер (до ~30 строк G-кода, см. MAX_BYTES);
//! критерий: parse не должен паниковать/падать — любые отказы обязаны
//! выражаться в Vec<Diagnostic> (SPEC §8). Запуск — fuzz/README.md.
#![no_main]

use eid_core::adapter::Adapter;
use eid_linuxcnc::LinuxCncAdapter;
use libfuzzer_sys::fuzz_target;

/// Граница буфера: ~30 строк по ~1 КиБ — гигантские входы лишь замедляют
/// мутации, интересующие ветки парсера достижимы на коротких программах.
const MAX_BYTES: usize = 30 * 1024;

fuzz_target!(|data: &[u8]| {
    if data.len() > MAX_BYTES {
        return;
    }
    // lossy: лексеры адаптеров принимают &str; не-UTF8 байты — тоже ветка
    // входа (диагностика, не паника).
    let text = String::from_utf8_lossy(data);
    let adapter = LinuxCncAdapter::new();
    let _ = adapter.parse(&text, "fuzz.linuxcnc.ngc");
});
