//! rs274_compare.rs — каноническое сравнение motion-потоков через LinuxCNC
//! интерпретатор rs274 (критерий 8, итерация 5): оригинальный и переведённый
//! файл прогоняются через `rs274 -g` (образ eid-linuxcnc-sim), канонические
//! выводы парсятся в последовательность CanonLine и сравниваются с допусками
//! EQUIVALENCE §4 (позиция/feed/аргументы — tol, символьные — точно).
//!
//! Сравнивается motion-подмножество канонического вывода (STRAIGHT_TRAVERSE,
//! STRAIGHT_FEED, ARC_FEED, SET_FEED_RATE, DWELL, SET_G92_OFFSET, SET_G5X_OFFSET,
//! SET_XY_ROTATION, USE_LENGTH_UNITS, SET_FEED_REFERENCE); meta-строки
//! (SELECT_PLANE / SET_MOTION_CONTROL_MODE / COMMENT / PROGRAM_END / ON_RESET /
//! SET_FEED_MODE / STOP_SPINDLE_TURNING / SET_SPINDLE_MODE / MESSAGE)
//! исключены: path_mode lossy (Q3) и COMMENT — документированные различия
//! уровней адаптеров (CORPUS lossy-класс), не движения.
//!
//! Docker-зависимость: каталог входного файла монтируется в /work
//! (Docker Desktop принимает Windows-путь в -v слева).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, PartialEq)]
pub enum ArgVal {
    Num(f64),
    Sym(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonLine {
    pub op: String,
    pub args: Vec<ArgVal>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CanonDivergence {
    pub index: usize,
    pub detail: String,
}

/// motion-подмножество канонического вывода (все остальное — meta/provenance).
const KEEP_OPS: [&str; 11] = [
    "STRAIGHT_TRAVERSE",
    "STRAIGHT_FEED",
    "ARC_FEED",
    "SET_FEED_RATE",
    "DWELL",
    "PAUSE",
    "SET_G92_OFFSET",
    "SET_G5X_OFFSET",
    "SET_XY_ROTATION",
    "USE_LENGTH_UNITS",
    "SET_FEED_REFERENCE",
];

/// Парсер канонического вывода rs274: строки `NN N..... OP(arg, arg, ...)`.
/// Числа → ArgVal::Num (f64), остальные → ArgVal::Sym; non-KEEP — пропущены.
pub fn parse_canonical(text: &str) -> Vec<CanonLine> {
    let mut out = Vec::new();
    for raw in text.lines() {
        let line = raw.trim();
        // формат: "<num> N..... OP(args)" (N-точек может быть 5)
        let Some(npos) = line.find("N.....") else {
            continue;
        };
        let call = line[npos + "N.....".len()..].trim();
        let Some(open) = call.find('(') else { continue };
        let Some(close_r) = call.rfind(')') else {
            continue;
        };
        if close_r < open {
            continue;
        }
        let op = call[..open].trim().to_string();
        if !KEEP_OPS.contains(&op.as_str()) {
            continue;
        }
        let args_text = &call[open + 1..close_r];
        let mut args = Vec::new();
        for piece in args_text.split(',') {
            let piece = piece.trim();
            if piece.is_empty() {
                continue;
            }
            match piece.parse::<f64>() {
                Ok(v) => args.push(ArgVal::Num(v)),
                Err(_) => args.push(ArgVal::Sym(piece.to_string())),
            }
        }
        out.push(CanonLine { op, args });
    }
    out
}

/// Счётчик уникальных имён канонического вывода (#28): два rs274-прогона с
/// одинаковым stem (параллельные тесты, один и тот же вход в разных потоках
/// или процессах) перезаписывали бы общий `<stem>.out` — гонка чтения/записи.
/// Имя `<stem>.<pid>.<seq>.out` уникально и между потоками (seq), и между
/// процессами (pid).
static OUT_SEQ: AtomicU64 = AtomicU64::new(0);

fn unique_out_name(stem: &str) -> String {
    let seq = OUT_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{stem}.{}.{}.out", std::process::id(), seq)
}

/// Drop-гард канонического вывода (#28): файл удаляется после чтения по
/// любому пути выхода функции (Ok/Err/unwind) — временные `.out` не
/// накапливаются в stage-каталоге; отсутствие файла при drop не ошибка.
struct OutFileGuard(PathBuf);

impl Drop for OutFileGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Прогон файла через rs274 (docker eid-linuxcnc-sim). Входной файл должен
/// лежать в каталоге, монтируемом в /work; канонический вывод пишется
/// контейнером в уникальное имя `<stem>.<pid>.<seq>.out` там же (#28), читается
/// и удаляется (OutFileGuard). Возвращает текст канонического вывода.
pub fn run_rs274(ngc_path: &Path) -> Result<String, String> {
    let dir = ngc_path
        .parent()
        .ok_or_else(|| "file has no parent dir".to_string())?
        .to_path_buf();
    let file_name = ngc_path
        .file_name()
        .ok_or_else(|| "file has no name".to_string())?
        .to_string_lossy()
        .into_owned();
    let stem = PathBuf::from(&file_name)
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .ok_or_else(|| "file has no stem".to_string())?;
    let out_name = unique_out_name(&stem);
    // Guard ставится ДО docker-прогона: rs274 пишет частичный вывод даже при
    // ненулевом exit (отказ разбора) — файл обязан удалиться по любому пути
    // выхода (Ok/Err/unwind). Именованный биндинг (не `let _`): guard обязан
    // жить до конца функции.
    let _guard = OutFileGuard(dir.join(&out_name));

    // #9: Docker Desktop на Windows требует Windows-путь слева от ':' в -v
    // (без замены путь с `/` тоже обычно принимается, но исторически монтируем
    // с `\` — поведение на Windows не менялось с итерации 5). На Unix замена
    // `/` → `\` ломает путь (там `\` — валидный символ имени файла), поэтому
    // путь передаётся как есть.
    #[cfg(windows)]
    let dir_str = dir.to_string_lossy().replace('/', "\\");
    #[cfg(not(windows))]
    let dir_str = dir.to_string_lossy().into_owned();
    let mount = format!("{dir_str}:/work");
    let in_vm = format!("/work/{file_name}");
    let out_vm = format!("/work/{out_name}");

    let output = Command::new("docker")
        .args([
            "run",
            "--rm",
            "-v",
            &mount,
            "eid-linuxcnc-sim",
            "rs274",
            "-g",
            &in_vm,
            &out_vm,
        ])
        .output()
        .map_err(|e| format!("docker run failed: {e}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        return Err(format!(
            "rs274 failed ({}): {} {}",
            output.status, stderr, stdout
        ));
    }
    let out_path = dir.join(&out_name);
    std::fs::read_to_string(&out_path)
        .map_err(|e| format!("canonical output read {}: {e}", out_path.display()))
}

/// Прогон и сравнение двух файлов: 0 divergences ⇔ канонические motion-потоки
/// эквивалентны (числа с допуском `tol` мм/units, символьные — точно).
pub fn compare_files(a: &Path, b: &Path, tol: f64) -> Result<Vec<CanonDivergence>, String> {
    let ca = parse_canonical(&run_rs274(a)?);
    let cb = parse_canonical(&run_rs274(b)?);
    Ok(compare_canonical(&ca, &cb, tol))
}

/// Сравнение канонических последовательностей: длина, имена опов, числа с
/// допуском `tol`, символьные аргументы — точно.
pub fn compare_canonical(a: &[CanonLine], b: &[CanonLine], tol: f64) -> Vec<CanonDivergence> {
    let mut div = Vec::new();
    if a.len() != b.len() {
        div.push(CanonDivergence {
            index: 0,
            detail: format!("длина канонического потока: {} vs {}", a.len(), b.len()),
        });
    }
    for (i, (x, y)) in a.iter().zip(b.iter()).enumerate() {
        if x.op != y.op {
            div.push(CanonDivergence {
                index: i,
                detail: format!("op mismatch: {} vs {}", x.op, y.op),
            });
            continue;
        }
        if x.args.len() != y.args.len() {
            div.push(CanonDivergence {
                index: i,
                detail: format!(
                    "{}: arity mismatch {} vs {}",
                    x.op,
                    x.args.len(),
                    y.args.len()
                ),
            });
            continue;
        }
        for (k, (av, bv)) in x.args.iter().zip(y.args.iter()).enumerate() {
            match (av, bv) {
                (ArgVal::Num(xn), ArgVal::Num(yn)) => {
                    if (xn - yn).abs() > tol {
                        div.push(CanonDivergence {
                            index: i,
                            detail: format!(
                                "{}: arg[{k}] {} vs {} (Δ = {:.6} > {tol})",
                                x.op,
                                xn,
                                yn,
                                (xn - yn).abs()
                            ),
                        });
                    }
                }
                (ArgVal::Sym(xs), ArgVal::Sym(ys)) => {
                    if xs != ys {
                        div.push(CanonDivergence {
                            index: i,
                            detail: format!("{}: arg[{k}] {xs} vs {ys}", x.op),
                        });
                    }
                }
                (ArgVal::Num(_), ArgVal::Sym(_)) | (ArgVal::Sym(_), ArgVal::Num(_)) => {
                    div.push(CanonDivergence {
                        index: i,
                        detail: format!("{}: arg[{k}] тип: число vs символ", x.op),
                    });
                }
            }
        }
    }
    div
}

/// docker доступен? (для guard в тестах без docker-окружения)
pub fn docker_available() -> bool {
    Command::new("docker")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #28: имена вывода уникальны в пределах процесса (seq) и содержат pid
    /// (уникальность между процессами) — параллельные прогоны с одним stem
    /// не перезаписывают чужой вывод.
    #[test]
    fn unique_out_names_dont_collide() {
        let a = unique_out_name("part");
        let b = unique_out_name("part");
        assert_ne!(a, b, "два вызова — разные имена");
        assert!(a.starts_with("part.") && a.ends_with(".out"), "{a}");
        assert!(
            a.contains(&std::process::id().to_string()),
            "в имени нет pid: {a}"
        );
    }

    /// #28: OutFileGuard удаляет файл на drop, отсутствие файла — не ошибка.
    #[test]
    fn out_file_guard_removes_file_and_tolerates_missing() {
        let dir = std::env::temp_dir();
        let p = dir.join(format!("eid_guard_test.{}.out", std::process::id()));
        std::fs::write(&p, "canon").unwrap();
        {
            let _g = OutFileGuard(p.clone());
        }
        assert!(!p.exists(), "guard обязан удалить файл");
        // повторный drop по несуществующему пути — тихо (remove_file игнорируется)
        drop(OutFileGuard(p.clone()));
    }

    /// #28 end-to-end (docker-gated, изолированный temp-каталог — устойчив к
    /// параллельным прогонам других тестов): run_rs274 не оставляет `.out` в
    /// каталоге входа ни на Ok-пути, ни на Err-пути (rs274 пишет частичный
    /// вывод даже при ненулевом exit — guard установлен до docker-прогона).
    #[test]
    fn run_rs274_leaves_no_out_files() {
        if !docker_available() {
            eprintln!("SKIP: docker недоступен");
            return;
        }
        let dir = std::env::temp_dir().join(format!("eid_rs274_leak_{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("temp stage dir");
        let ok = dir.join("leak_ok.ngc");
        std::fs::write(&ok, "%\nG21\nG0 X1\nG1 X2 F100\nM2\n%").unwrap();
        let bad = dir.join("leak_bad.ngc");
        std::fs::write(&bad, "%\nG999 ZZZQQ\n%").unwrap();

        let r_ok = run_rs274(&ok);
        let r_bad = run_rs274(&bad); // Err-путь: частичный .out обязан удалиться
        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .filter(|n| n.ends_with(".out"))
            .collect();
        let _ = std::fs::remove_dir_all(&dir);
        eprintln!("run_rs274: ok={r_ok:?} bad={r_bad:?}");
        assert!(leftovers.is_empty(), "утечка .out: {leftovers:?}");
        assert!(
            r_bad.is_err(),
            "мусорный вход обязан давать Err (interp отказ)"
        );
    }
}
