//! Параллельный прогон корпуса поверх `Adapter: Send + Sync` — демо R10
//! (INTERFACE-V0.2-DESIGN §7 реестр рисков R10; возможность #22, W2 v0.2.0
//! / 53a4b79: owning-результат вместо RefCell-буферов открыл Sync).
//!
//! Тесты:
//!  * `parallel_corpus_smoke` — ПОСТОЯННЫЙ (не ignored): половина Tier B
//!    (stride-нечётные, 72 файла), последовательный vs параллельный прогон,
//!    отчёты бит-в-бит равны. Быстрый (~2–3с debug) — см. шапку.
//!  * `parallel_corpus_parity_sequential` — ПОСТОЯННЫЙ: полный Tier B (144
//!    файла), два прогона в одном тесте; gold standard = последовательный
//!    отчёт, параллельный бит-в-бит равен (entries + счётчики + коды отказов).
//!    ~4–6с debug — приемлем для дефолт-прогона (норма крейта: full_report
//!    5с, rs274_pilot 11с); НЕ ignored, т.к. паритет — главное доказательство
//!    R10, а время укладывается в существующую дисциплину дефолт-прогона.
//!  * `parallel_shared_arc_identity` — ПОСТОЯННЫЙ: один объект адаптера во
//!    всех потоках (указатель `Arc::as_ptr` совпадает) — «не клон на поток».
//!  * `parallel_same_file_concurrent_parse` — ПОСТОЯННЫЙ: 8 потоков parse
//!    ОДНОГО файла через ОДИН `&Arc<dyn Adapter>` — все исходы идентичны.
//!  * `parallel_tier_b_speed_report` — #[ignore] (бенчмарк, не гейт):
//!    wall-time seq-vs-par таблица для отчёта. Замеры (debug, 16 лог. ядер,
//!    тихая машина): полный Tier B seq 3.4s → par(16) 1.55s = 2.22× — предел
//!    Амдала по самому большому файлу (3.5MB grbl, 1.48s = критический путь);
//!    мелкие 141 файл 0.78s → 0.19s = 4.06×.
//!
//! Thread-паттерн (что демонстрируется и что НЕ обещает Sync):
//!  * один `Registry` (внутри `Arc<dyn Adapter>`) создаётся РАЗ; скоуп-потоки
//!    захватывают его по `&`-ссылке — компилятор требует `Registry: Sync`
//!    ⇔ `Arc<dyn Adapter>: Send + Sync` (supertrait v0.2). До #22 (`RefCell`)
//!    этот модуль не собрался бы — R10 и был «риском-возможностью».
//!  * lookup возвращает `&Arc<dyn Adapter>` из общего реестра: ни нового
//!    адаптера на поток, ни даже `Arc::clone` — `parse(&self)` зовётся
//!    параллельно на ОДНОМ объекте.
//!  * Чего Sync НЕ даёт: взаимного исключения или per-parse состояния.
//!    Owning-результат `ParseOutput` (v0.2 #22) убирает разделяемое
//!    изменяемое состояние как класс — потому паритет выполняется без
//!    локов вокруг parse. Локи в runner-е — только на запись РЕЗУЛЬТАТОВ
//!    (по индексам, без пересечений), не на адаптер.

use eid_grbl::GrblAdapter;
use eid_haas::HaasAdapter;
use eid_harness::manifest::{self, Manifest};
use eid_harness::parallel::run_tier_b_parallel;
use eid_harness::runner::{run_tier_b, Registry};
use eid_linuxcnc::LinuxCncAdapter;
use std::collections::BTreeMap;
use std::sync::Arc;

// ===========================================================================
// Хелперы
// ===========================================================================

fn corpus_manifest() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../corpus/manifest.toml")
}

fn load_manifest() -> Manifest {
    manifest::load(&corpus_manifest()).expect("manifest.toml loads")
}

fn n_threads() -> usize {
    std::thread::available_parallelism()
        .map(std::num::NonZeroUsize::get)
        .unwrap_or(1)
}

/// Полный registry итерации 23 (как в full_report.rs): eid-l1, grbl-1.1,
/// linuxcnc, fanuc (register_as — ключ манифеста tier_b), haas (ключ CLI).
fn full_registry() -> Registry {
    let mut reg = Registry::new();
    reg.register(Arc::new(eid_core::adapter::EidTextAdapter::new()));
    reg.register(Arc::new(GrblAdapter::new()));
    reg.register(Arc::new(LinuxCncAdapter::new()));
    reg.register_as("fanuc", Arc::new(eid_fanuc::FanucAdapter::new()));
    reg.register_as("haas", Arc::new(HaasAdapter::new()));
    reg
}

/// Под-манифест: только выбранные tier_b-файлы (vectors не участвуют в
/// tier_b-прогонах — пусто). Порядок файлов сохранён → отчёты сравнимы.
fn subset_manifest(m: &Manifest, keep: impl Fn(usize) -> bool) -> Manifest {
    Manifest {
        corpus_version: m.corpus_version.clone(),
        vectors: Vec::new(),
        tier_b_files: m
            .tier_b_files
            .iter()
            .enumerate()
            .filter(|(i, _)| keep(*i))
            .map(|(_, f)| f.clone())
            .collect(),
    }
}

/// Таймированный прогон (после разминки вызывающим кодом).
fn timed<R>(f: impl FnOnce() -> R) -> (R, std::time::Duration) {
    let t = std::time::Instant::now();
    let r = f();
    (r, t.elapsed())
}

fn secs(d: std::time::Duration) -> f64 {
    d.as_secs_f64()
}

// ===========================================================================
// 1. Смоук (постоянный): половина Tier B, parity seq-vs-par
// ===========================================================================

#[test]
fn parallel_corpus_smoke() {
    let m = load_manifest();
    let reg = full_registry();
    let extra = BTreeMap::new();
    let threads = n_threads();

    // Детерминированная половина: нечётные индексы (72 из 144; включает
    // большой fanuc RTCP-файл — индекс 141 — и 71 мелкий).
    let half = subset_manifest(&m, |i| i % 2 == 1);
    assert!(
        half.tier_b_files.len() >= m.tier_b_files.len() / 2,
        "половина корпуса: {} из {}",
        half.tier_b_files.len(),
        m.tier_b_files.len()
    );

    // Gold standard: последовательный прогон под-множества.
    let (seq, seq_t) = timed(|| run_tier_b(&half, &reg, &extra));
    // Параллельный прогон (available_parallelism).
    let (par, par_t) = timed(|| run_tier_b_parallel(&half, &reg, &extra, threads));
    // Перекрёстная детерминированность: другое число потоков — тот же отчёт.
    let par2 = run_tier_b_parallel(&half, &reg, &extra, 2);

    assert_eq!(seq, par, "бит-в-бит паритет при threads={threads}");
    assert_eq!(seq, par2, "бит-в-бит паритет при threads=2");
    assert!(par.is_crash_free(), "crash=0 инвариант (CORPUS §3)");
    assert_eq!(par.total, half.tier_b_files.len());
    assert_eq!(
        par.entries.len(),
        par.total,
        "исход по каждому файлу ровно один"
    );
    println!(
        "parallel_corpus_smoke: files={} threads={threads} seq={:.3}s par={:.3}s \
         ok={} no_adapter={} refused={:?}",
        par.total,
        secs(seq_t),
        secs(par_t),
        par.parse_ok,
        par.no_adapter,
        par.refused_by_code
    );
}

// ===========================================================================
// 2. Полный parity (постоянный): 144 файла, два прогона в одном тесте
// ===========================================================================

#[test]
fn parallel_corpus_parity_sequential() {
    let m = load_manifest();
    let reg = full_registry();
    let extra = BTreeMap::new();
    let threads = n_threads();
    // Дисциплина счётчиков CORPUS R-1 (как в harness_tests.rs): никаких
    // захардкоженных количеств — сейчас в манифесте 144 файла, корпус растёт.
    let total = m.tier_b_files.len();
    assert!(total >= 144, "sanity: полный Tier B (сейчас {total})");

    // --- Прогон 1: последовательный (gold standard) --------------------------
    let seq = run_tier_b(&m, &reg, &extra);

    // --- Прогон 2: параллельный, available_parallelism -----------------------
    let par = run_tier_b_parallel(&m, &reg, &extra, threads);

    // Бит-в-бит: entries (путь, диалект, строка исхода — включая точный список
    // кодов отказов в Err(...)), счётчики Ok/no-adapter/panic, refused_by_code.
    assert_eq!(seq, par, "полный Tier B: отчёты бит-в-бит равны");
    assert!(seq.is_crash_free());
    assert!(par.is_crash_free());
    assert_eq!(seq.entries, par.entries);
    // Порядок entries = manifest-порядок (не порядок завершения потоков).
    for (i, e) in par.entries.iter().enumerate() {
        assert_eq!(
            e.path,
            m.tier_b_files[i].path.display().to_string(),
            "entries[{i}] в manifest-порядке"
        );
    }
    println!(
        "parallel_corpus_parity_sequential: files={} threads={threads} \
         ok={} no_adapter={} refused={:?} — бит-в-бит паритет ОК",
        total, par.parse_ok, par.no_adapter, par.refused_by_code
    );
}

// ===========================================================================
// 3. Один объект адаптера на все потоки (анти-паттерн «клон на поток» исключён)
// ===========================================================================

#[test]
fn parallel_shared_arc_identity() {
    let reg = full_registry();
    // Захват `&Registry` скоуп-потоками компилируется ТОЛЬКО при
    // `Registry: Sync` ⇔ `Arc<dyn Adapter>: Send + Sync` — сам факт сборки
    // этого теста есть compile-time доказательство supertrait v0.2.
    let ptrs: Vec<usize> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                s.spawn(|| {
                    // lookup из ОБЩЕГО реестра по &-ссылке (никаких new()/clone)
                    let a = reg.get("grbl-1.1").expect("grbl-1.1 в registry");
                    Arc::as_ptr(a) as *const () as usize
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("thread ok"))
            .collect()
    });
    assert_eq!(ptrs.len(), 8);
    assert!(
        ptrs.windows(2).all(|w| w[0] == w[1]),
        "все потоки видят ОДИН объект адаптера: {ptrs:?}"
    );
    // Дополнительно: fanuc (register_as) и haas — тоже один объект.
    let fanuc: Vec<usize> = std::thread::scope(|s| {
        (0..4)
            .map(|_| {
                s.spawn(|| {
                    Arc::as_ptr(reg.get("fanuc").expect("fanuc в registry")) as *const () as usize
                })
            })
            .collect::<Vec<_>>()
            .into_iter()
            .map(|h| h.join().unwrap())
            .collect()
    });
    assert!(fanuc.windows(2).all(|w| w[0] == w[1]));
}

// ===========================================================================
// 4. Стресс: 8 потоков парсят ОДИН файл через ОДИН &Arc<dyn Adapter>
// ===========================================================================

#[test]
fn parallel_same_file_concurrent_parse() {
    let m = load_manifest();
    let reg = full_registry();
    // Большой fanuc RTCP (44629 строк): реальная нагрузка на &self.
    let rtcp = m
        .tier_b_files
        .iter()
        .find(|f| f.path.display().to_string().contains("rtcp"))
        .expect("RTCP-файл в манифесте");
    let text = std::fs::read_to_string(&rtcp.path).expect("RTCP читается");
    let adapter = reg
        .get(&rtcp.dialect)
        .or_else(|| {
            // fanuc в registry под ключом "fanuc" (register_as), диалект записи — "fanuc"
            reg.get("fanuc")
        })
        .expect("адаптер fanuc в registry");

    // До v0.2 (RefCell-буферы, !Sync) этот scope не собрался бы компилятором.
    let results: Vec<String> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                // НЕ move: все 8 замыканий заимствуют ОДИН text и ОДИН adapter
                // (&Arc — Copy, text — заимствование из внешнего скоупа).
                s.spawn(|| match adapter.parse(&text, "rtcp.concurrent") {
                    Ok(out) => format!(
                        "Ok({} ops, {} notices)",
                        out.program.ops.len(),
                        out.notices.len()
                    ),
                    Err(diags) => format!(
                        "Err({})",
                        diags
                            .iter()
                            .map(|d| d.code.as_str())
                            .collect::<Vec<_>>()
                            .join(",")
                    ),
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("thread ok"))
            .collect()
    });
    // Все 8 исходов идентичны (и равны последовательному эталону).
    let reference = match adapter.parse(&text, "rtcp.concurrent") {
        Ok(out) => format!(
            "Ok({} ops, {} notices)",
            out.program.ops.len(),
            out.notices.len()
        ),
        Err(diags) => format!(
            "Err({})",
            diags
                .iter()
                .map(|d| d.code.as_str())
                .collect::<Vec<_>>()
                .join(",")
        ),
    };
    assert_eq!(rtcp.dialect, "fanuc");
    assert!(
        results.iter().all(|r| r == &reference),
        "все потоки: {results:?}, эталон: {reference}"
    );
    println!(
        "parallel_same_file_concurrent_parse: 8 потоков × RTCP {} строк → {reference}",
        text.lines().count()
    );
}

// ===========================================================================
// 5. Скорость (#[ignore]: бенчмарк, не приёмочный гейт)
//    cargo test -p eid-harness --test parallel_tier_b \
//        parallel_tier_b_speed_report -- --ignored --nocapture
// ===========================================================================

#[test]
#[ignore = "бенчмарк wall-time seq-vs-par (сырьё для R10-отчёта), не приёмочный гейт"]
fn parallel_tier_b_speed_report() {
    let m = load_manifest();
    let reg = full_registry();
    let extra = BTreeMap::new();
    let n = n_threads();
    let big = [
        "ncm_g434_rtcp_5x_milling.nc",
        "ncm_g43_g69_3d5x_milling.nc",
        "vis_SmilinJack",
    ];
    let is_big = |f: &eid_harness::manifest::TierBFile| {
        big.iter().any(|b| f.path.display().to_string().contains(b))
    };

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!("available_parallelism = {n}; profile = {profile} (cargo test)");

    // --- Полный Tier B -------------------------------------------------------
    let gold = run_tier_b(&m, &reg, &extra); // разминка + эталон
    let (seq, seq_t) = timed(|| run_tier_b(&m, &reg, &extra));
    assert_eq!(seq, gold);
    println!("\n== Полный Tier B ({} файлов) ==", m.tier_b_files.len());
    println!("| конфиг | wall, s | x к seq |");
    println!("|---|---:|---:|");
    println!("| seq (1 поток) | {:.3} | 1.00 |", secs(seq_t));
    let mut par_n = 0.0;
    for threads in [2usize, 4, 8, n].into_iter().filter(|t| *t > 1) {
        let _ = run_tier_b_parallel(&m, &reg, &extra, threads); // разминка
        let (par, t) = timed(|| run_tier_b_parallel(&m, &reg, &extra, threads));
        assert_eq!(par, gold, "паритет при threads={threads}");
        println!(
            "| par({threads}) | {:.3} | {:.2} |",
            secs(t),
            secs(seq_t) / secs(t)
        );
        par_n = secs(t);
    }

    // --- Только большие файлы (top-3: 8.2MB) ---------------------------------
    let big_m = subset_manifest(&m, |i| is_big(&m.tier_b_files[i]));
    assert_eq!(big_m.tier_b_files.len(), 3, "три больших файла");
    let gold_big = run_tier_b(&big_m, &reg, &extra);
    let (_, big_seq) = timed(|| run_tier_b(&big_m, &reg, &extra));
    let _ = run_tier_b_parallel(&big_m, &reg, &extra, n);
    let (big_par_rep, big_par) = timed(|| run_tier_b_parallel(&big_m, &reg, &extra, n));
    assert_eq!(big_par_rep, gold_big, "паритет на больших файлах");
    println!(
        "\n== Только большие файлы ({} шт) ==",
        big_m.tier_b_files.len()
    );
    for f in &big_m.tier_b_files {
        println!(
            "| {} | {} | {} байт |",
            f.dialect,
            f.path.display(),
            std::fs::metadata(&f.path).map(|md| md.len()).unwrap_or(0)
        );
    }
    println!(
        "| seq | {:.3} s | 1.00 |\n| par({n}) | {:.3} s | {:.2} |",
        secs(big_seq),
        secs(big_par),
        secs(big_seq) / secs(big_par)
    );

    // --- Только мелкие файлы (141 шт, без top-3) -----------------------------
    let small_m = subset_manifest(&m, |i| !is_big(&m.tier_b_files[i]));
    assert_eq!(
        small_m.tier_b_files.len(),
        m.tier_b_files.len() - 3,
        "мелкие = все минус top-3"
    );
    let gold_small = run_tier_b(&small_m, &reg, &extra);
    let (_, small_seq) = timed(|| run_tier_b(&small_m, &reg, &extra));
    let _ = run_tier_b_parallel(&small_m, &reg, &extra, n);
    let (small_par, small_par_t) = timed(|| run_tier_b_parallel(&small_m, &reg, &extra, n));
    assert_eq!(small_par, gold_small, "паритет на мелких файлах");
    println!(
        "\n== Только мелкие файлы ({} шт) ==",
        small_m.tier_b_files.len()
    );
    println!(
        "| seq | {:.3} s | 1.00 |\n| par({n}) | {:.3} s | {:.2} |",
        secs(small_seq),
        secs(small_par_t),
        secs(small_seq) / secs(small_par_t)
    );

    // --- По-файловая таблица больших (кто доминирует) ------------------------
    println!("\n== По-файловая таблица больших файлов (seq) ==");
    for f in &big_m.tier_b_files {
        let one = subset_manifest(&m, |i| m.tier_b_files[i].path == f.path);
        let _ = run_tier_b(&one, &reg, &extra);
        let (_, t) = timed(|| run_tier_b(&one, &reg, &extra));
        println!(
            "| {} | {:.3} s |",
            f.path.file_name().unwrap_or_default().to_string_lossy(),
            secs(t)
        );
    }

    println!(
        "\nКонтроль: полный par({n}) = {par_n:.3}s; ожидание для честного отчёта: \
         выигрыш ограничен тремя большими файлами (работа ~8.2MB распределяется \
         максимум на 3 ядра), мелкие файлы дают шум/overhead."
    );
}
