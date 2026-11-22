//! ЗАГОТОВКА (этап 12.1, агент A) — ПОЛНЫЙ Pass-1 lowering L2 → L1.
//! Заполняется в ФАЗЕ 12.2 (следующий заход).
//!
//! В 12.1 минимальный lowering живёт в [`crate::interp`] (D0–D2):
//! константные Rapid/Linear (без переменных и ротационных осей) → L1-опы,
//! всё с переменными / осями A–C / трансформациями → носители
//! [`crate::interp::RawTailItem`] (`Op::Raw` + маркер `RawMarker`).
//!
//! ФАЗА 12.2 забирает lowering сюда и закрывает (§3 матрица дизайн-дока):
//! * параметрические ходы (разрешение переменных в момент исполнения),
//! * ротационные оси A/B/C + RTCP (`RtcpMode`, G43.4/G43.5),
//! * трансформации (`TransformKind`: Rotate2D/Scale/Mirror/TiltedWorkPlane),
//! * Arc-геометрия (i/j/k/r → center/normal/sweep_deg),
//! * IfGoto (таблица меток N/O), модальные вызовы G66,
//! * дублирование развёрток (`sub` в L1-опах).
//!
//! ОЖИДАЕМАЯ ТОЧКА ВХОДА (согласовать с 12.2):
//! ```text
//! pub fn lower(program: &L2Program, cfg: L2Config) -> Result<LoweringResult, L2Error>
//! ```

// Пусто: фаза 12.2. Интерфейс LoweringResult/RawMarker см. crate::interp.
