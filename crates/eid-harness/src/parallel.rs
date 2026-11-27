//! parallel — параллельный прогон Tier B (CORPUS §3) поверх `Adapter:
//! Send + Sync` (interface v0.2, W2 53a4b79; демо риска R10 /
//! INTERFACE-V0.2-DESIGN §7, возможность #22).
//!
//! Что здесь демонстрируется:
//!  * **Sync — факт компилятора, не надежда**: скоуп-потоки захватывают
//!    `&Registry` (внутри — `Arc<dyn Adapter>`) по ссылке. Это компилируется
//!    только при `Registry: Sync` ⇔ `Arc<dyn Adapter>: Send + Sync` —
//!    supertrait-гарантия trait v0.2 (до #22 `RefCell` делал адаптер `!Sync`
//!    — этот модуль не собрался бы).
//!  * **Паттерн «один адаптер на все потоки»**: ни одного `GrblAdapter::new()`
//!    на поток, ни одного `Arc::clone` — все потоки зовут `parse(&self)`
//!    ОДНОГО объекта через `&Arc` из общего реестра (проверка идентичности
//!    объекта — тест `parallel_shared_arc_identity` в tests/parallel_tier_b.rs).
//!    Чего Sync НЕ даёт: если бы адаптер нёс изменяемое состояние с семантикой
//!    «последний parse побеждает» (буферы v0.1) — паритет бы не выполнился;
//!    owning-результат ParseOutput (v0.2 #22) убирает само состояние.
//!  * **Корректность**: per-file логика — та же [`run_tier_b_file`], что и в
//!    последовательном [`run_tier_b`]; результаты собираются по индексам в
//!    manifest-порядке → отчёты бит-в-бит равны (parity-тесты — gold standard).
//!
//! Почему `std::thread::scope`, не rayon: demo-путь не добавляет зависимостей
//! (workspace почти zero-deps; harness несёт только serde+toml QA-контура).
//! Работа — 144 файла с тяжелым хвостом (3 файла ~8.2MB доминируют), поэтому
//! балансировка — борт-очередь на `AtomicUsize` (кто свободен — берёт следующий
//! индекс), а не статическое разбиение на чанки. 144 lock-захватов на файл —
//! шум против миллисекунд парсинга.

use crate::manifest::Manifest;
use crate::runner::{fold_tier_b, run_tier_b_file, Registry, TierBFileDelta, TierBReport};
use eid_core::adapter::Adapter;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

/// Параллельный прогон Tier B: parse-or-diagnose на `threads` потоках.
///
/// Отчёт бит-в-бит равен последовательному [`crate::runner::run_tier_b`]
/// (тот же per-file код-путь, тот же порядок entries — manifest-порядок;
/// `refused_by_code` — BTreeMap, свёртка коммутативна). `threads <= 1` или
/// `threads > файлов` — деградация в последовательный путь (та же семантика).
pub fn run_tier_b_parallel(
    manifest: &Manifest,
    adapters: &Registry,
    extra: &BTreeMap<String, Arc<dyn Adapter>>,
    threads: usize,
) -> TierBReport {
    let files = &manifest.tier_b_files;
    if files.is_empty() {
        return crate::runner::run_tier_b(manifest, adapters, extra);
    }
    let threads = threads.clamp(1, files.len());
    if threads <= 1 {
        // Один поток = последовательный путь (без пустой посуды).
        return crate::runner::run_tier_b(manifest, adapters, extra);
    }

    // Общий ИММУТАБЕЛЬНЫЙ view: `&Registry` и `&BTreeMap` захватываются
    // скоуп-потоками по ссылке (компилятор: `&Registry: Send` ⇔ `Registry:
    // Sync` — supertrait-гарантия trait v0.2). Lookup внутри run_tier_b_file
    // возвращает `&Arc<dyn Adapter>` прямо из реестра — один и тот же объект
    // адаптера у всех потоков, ни `Arc::clone`, ни per-thread конструирования.
    // Борт-очередь: следующий необработанный индекс файла.
    let next = AtomicUsize::new(0);
    // Результаты по индексам (каждый поток пишет ТОЛЬКО свой индекс —
    // data race невозможен по построению; Mutex — на время записи).
    let deltas: Mutex<Vec<Option<TierBFileDelta>>> =
        Mutex::new((0..files.len()).map(|_| None).collect());

    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= files.len() {
                    break;
                }
                // Паника парсинга ловится внутри run_tier_b_file (crash=0,
                // CORPUS §3) — поток живёт до конца очереди.
                let delta = run_tier_b_file(&files[i], adapters, extra);
                deltas.lock().expect("results mutex")[i] = Some(delta);
            });
        }
    });

    let deltas: Vec<TierBFileDelta> = deltas
        .into_inner()
        .expect("results mutex")
        .into_iter()
        .map(Option::unwrap) // каждый индекс записан ровно один раз
        .collect();
    fold_tier_b(deltas)
}
