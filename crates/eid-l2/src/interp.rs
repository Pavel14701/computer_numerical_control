//! EID/L2 — интерпретатор D0–D3 + полный Pass-1 lowering (этап 12.2, агент A).
//!
//! УРОВНИ РАЗРЕШИМОСТИ (§4 дока EID-L2-DESIGN v0.1; D4–D5 — фаза 12.3, smt.rs):
//! * **D0** — константная свёртка/распространение: `Assign` (копирование
//!   Null), `Move` с ПОЛНЫМ разрешением слов ([`pose_resolve`]: слово со
//!   значением Null = слово отсутствует — семантика адресных слов,
//!   таблица 2.1; наследование координат по модальным словам — трекинг
//!   программной позиции), `L1Op`-passthrough. Параметрический ход теперь
//!   РАЗРЕШАЕТСЯ (§3 строка 1: `#100 = 5, G0 X#100` → `Op::Rapid X5.0000`),
//!   числа — single-final-rounding 4dp (SPEC §10.5, [`mk_num4`]).
//! * **D1** — локальные кадры G65/M98/M97: стек вызовов ГЛУБИНА 9
//!   ([`L2Config::max_call_depth`], дизайн §5 12.2; превышение →
//!   [`L2ExecError::CallDepthExceeded`] = E_LIMIT_EXCEEDED), args
//!   A→#1..Z→#26 с пробросом Null (таблица 2.1), repeat L/K-развёртка,
//!   возврат M99 ([`L2Stmt::Return`]) → pop кадра; О-конец без M99 —
//!   неявный return. Modal G66 — КАРКАС: регистрация + триггер «после
//!   motion» (см. ниже контракт G67).
//! * **D2** — детерминированные WHILE/IF-GOTO: CFG-исполнитель с таблицей
//!   меток ([`crate::lowering::collect_labels`] по узлам [`L2Stmt::Label`]
//!   и [`crate::ast::L2Program::labels`] парсера — контракт B2/D2),
//!   do_id ∈ 1..=3, cond через `eval` (Null-cond = false), лимит
//!   `max_loop_steps` НА КАЖДЫЙ цикл (T5) + общий бюджет интерпретатора
//!   (`max_eval_steps`).
//! * **D3** — мёртвые ветки/тривиальные условия: cond, вычислившийся в
//!   константу (IF [1 EQ 1]), разрешается CFG-исполнителем на месте —
//!   мёртвая ветка НЕ ВЫЧИСЛЯЕТСЯ вовсе (в т.ч. SysVar-чтения в ней не
//!   дают ложных остановов — синергия с D4, §4).
//!
//! LOWERING (§3 матрица, полная — механика в [`crate::lowering`]):
//! * параметрические Rapid/Linear → L1-опы с наследованием координат и
//!   трекингом позиции (start Known/Unknown — точки разрыва SPEC §2);
//! * дуги IJK/R → [`Op::Arc`] (канон eid-core: sweep (0°,360°], нормаль
//!   +axis плоскости, pitch = внеплоскостная компонента хода);
//! * ротационные оси (A/B/C) → [`Op::Raw`] с payload-кадром
//!   `G0 X15.0000 A15.0000\n` (T4 ДОСЛОВНО, hex с терминатором), код
//!   E_UNSUPPORTED_SEMANTICS (§3 строка 5) — режим [`RtcpMode::Off`];
//!   12.5.3/12.5.4 (фаза 12.5, MachineKinematics-профиль): при
//!   kinematics-профиле (все шесть топологий [`RtcpMode`] — trt/tdr/
//!   trsrn/maxkins/5axis, оракулы LinuxCNC — ядра `crate::kinematics*`)
//!   ходы с ПОЛНОЙ TCP-позой опускаются в L1-опы machine-осей: XYZ =
//!   inverse-кинематика топологии, ротационные слова = joints
//!   pass-through (table-table trt/tdr; head-table maxkins; gantry
//!   5axis) ИЛИ через angle-solver (head-head trsrn: B/C — ориентация
//!   инструмента → `trsrn_angles_from_tool_axis` → joints; таблица
//!   «pass-through vs solver» — док [`RtcpMode`]); носитель остаётся
//!   только на честных границах (неполная поза, чужое ротационное
//!   слово, дуги);
//! * G43.4/G49 RTCP, G68.2 TWP → Raw с подставленными переменными
//!   (§3 строка 6; TWP-тело подавлено);
//! * G31 / чтение SkipPos/AbsMachinePos/WorkPos → останов
//!   E_RUNTIME_STATE ([`L2ExecError::Eval`]), программа остаётся в L2;
//!   чтение WcsOffset → E_REQUIRES_MACHINE_TABLE
//!   ([`L2ExecError::MachineTableRequired`]); ModalGroup → снапшот
//!   модальностей ([`Modals`], разрешим статически);
//! * G68/G51 → аффинные трансформации ([`crate::lowering::Affine3`]) с
//!   умножением координат и центров дуг (векторы IJK — линейной частью);
//!   G51.1/G101 зеркало — дефолт-отказ E_UNSUPPORTED_SEMANTICS /
//!   `allow_mirror_bake` = true → инверсия + Cw↔Ccw + Notice (T6).
//!
//! КОНТРАКТ-ПРОБЕЛЫ (честно, для диспетчера/парсеров):
//! * G67 (отмена модального вызова) не имеет узла AST в §2.4 — каркас G66
//!   держит вызов активным до конца программы; парсерный контракт
//!   (например, аддитивный узел) — предмет интеграции;
//! * модальная плоскость G17/G18/G19 не имеет узла AST — снапшот держит
//!   power-on дефолт G17 (дуги в G18/G19 — после парсерного контракта);
//! * S-слово хода не пишется в `State.spindle` (направление Cw/Ccw
//!   требует M3/M4-контекста, которого в AST §2.4 нет) — 12.1-решение.
//!
//! ГВАРДЫ (T5): `max_eval_steps` — общий бюджет (узлы выражений +
//! операторы + итерации), `max_loop_steps` — на КАЖДУЮ сессию цикла
//! WHILE; бесконечный WHILE детерминированно → E_LIMIT_EXCEEDED.
//!
//! ПРИМЕЧАНИЕ (гонка волн, для диспетчера): mechanical-фиксы агента D2
//! в этом файле (no-op arm'ы Label/Return, labels: Default::default()
//! в тестах 12.1) ПОГЛОЩЕНЫ полной реализацией 12.2 — семантика
//! возврата/CFG-переходов здесь, как и было оговорено в их комментариях.

use crate::ast::{
    Axis, Expr, ExprPose, L2Program, L2Stmt, MoveKind, Plane, SubTarget, SysVar, TransformKind,
    Val, VarRef,
};
use crate::eval::{eval_expr, write_var, L2Env, L2Error};
use crate::kinematics::{xyzac_inverse, xyzbc_inverse, Pose6, TrtParams};
use crate::kinematics_5axis::{fiveaxis_inverse, FiveAxisParams, PoseXyzbcw};
use crate::kinematics_maxkins::{maxkins_inverse, MaxkinsParams, PoseMaxkins};
use crate::kinematics_tdr::{tdr_inverse, TdrParams};
use crate::kinematics_trsrn::{
    tool_rotation, trsrn_angles_from_tool_axis, trsrn_tcp_inverse, TrsrnParams,
};
use crate::lowering::{
    arc_from_ijk, arc_from_r, format_frame, plane_linear, raw_frame_op, resolve_labels,
    sub_target_name, Affine3, ArcGeomError,
};
use eid_core::ast::{Dir, Num, Op, Point3, StartPoint, State};
use eid_core::diag::{Diagnostic, ErrorCode, Stage};
use std::collections::{BTreeMap, HashMap};

// ===========================================================================
// Конфиг и ошибки
// ===========================================================================

/// RTCP-профиль Pass-1 lowering (12.5.2/12.5.3/12.5.4, фаза 12.5): как
/// интерпретатор обращается с ротационными словами (A/B/C) и TCP-позой.
///
/// РЕЖИМ OFF (дефолт = `MachineKinematics::Triv`, поведение 12.2/12.5.2):
/// кадр с ротационными словами НЕ опускается в L1-опы — ход по X/Y/Z+A/C
/// без пересчёта joints изменил бы TCP-геометрию (поворот стола двигает
/// TCP), поэтому ротационный кадр остаётся [`Op::Raw`]-носителем T4 с
/// ПОДСТАВЛЕННЫМИ значениями (переменные A/C-слов разрешаются так же,
/// как линейные — [`ResolvedPose`]); линейная компонента трекается
/// ([`Self::update_pos_prog`]). [`LoweringResult::rtcp_active`] = `false`.
///
/// АКТИВНЫЕ РЕЖИМЫ (программа говорит TCP-координатами — семантика
/// RTCP-контура станка; lowering переводит ходы в MACHINE-ОСИ):
/// * Rapid/Linear с полностью известной TCP-позой (слова + наследование,
///   [`Self::update_pos_prog`]) → L1-оп `Op::Rapid`/`Op::Linear`, где
///   X/Y/Z = joints (machine-координаты осей) из inverse-кинематики ядра
///   (`crate::kinematics*` — оракулы LinuxCNC, круговой инвариант
///   forward∘inverse ≡ identity — `tests/kinematics_tests.rs` +
///   `tests/kinematics_topologies_tests.rs`);
/// * НЕ полный ход → честный носитель (см. [`RawMarker`]);
/// * дуги: TCP-дуга при активной кинематике НЕ аффинно отображается в
///   machine-оси (5-осевая интерполяция) → честный носитель [`RawMarker::ArcUnlowered`]
///   (граница сохраняется во ВСЕХ режимах);
/// * чужое ротационное слово (буква вне топологии) → носитель.
///
/// # Семантика ротационных слов по топологиям («pass-through vs solver»)
///
/// | Режим | Слова = joints? | Линейная TCP → joints | Оракул |
/// |---|---|---|---|
/// | `XyzacTrt` | A/C pass-through (+con-знак в трансляции) | `xyzac_inverse` | trtfuncs.c |
/// | `XyzbcTrt` | B/C pass-through | `xyzbc_inverse` | trtfuncs.c |
/// | `XyzabTdr` | A/B pass-through (table-table cradle; знаки comp, без con-флага) | `tdr_inverse` | xyzab_tdr_kins.comp |
/// | `Trsrn` | A (стол) pass-through; **B/C — НЕ pass-through**: слова = ориентация ИНСТРУМЕНТА → вектор оси → **angle-solver** (`trsrn_angles_from_tool_axis`, главные ветви C=θ1∈[−90°,90°], B=θ2∈[0°,180°]) | `trsrn_tcp_inverse` на решённых углах | xyzacb_trsrn.comp + kins-twp python |
/// | `Maxkins` | A/B/C pass-through (A — пассивный джоинт оракула; U/V/W joints держатся home 0 — слова не адресуемы в L2-AST §2.4) | `maxkins_inverse` | maxkins.c |
/// | `FiveAxis` | B/C pass-through; W joint держится home 0 (W-слово не адресуемо в L2-AST) | `fiveaxis_inverse` | 5axiskins.c |
///
/// TRSRN-СОЛВЕР (ключевое отличие от table-табличных топологий): у
/// head-head/nutating головы джоинт secondary вращается о НАКЛОНЁННУЮ ось
/// нутации `(0, Sv, Cv)`, поэтому программная ориентация и machine-джоинты
/// — разные параметризации: lowering прогоняет программные B/C через
/// решатель углов (200/200 round-trip на решётке ν∈{30,45,60,90} —
/// `tests/kinematics_topologies_tests.rs::trsrn_angle_solver_round_trip_grid`).
/// На главных ветвях решатель восстанавливает программные углы точно
/// (семантика совпадает с pass-through); вне главных ветвей (B<0°,
/// C вне ±90°) joints канонизируются до главных (та же ось инструмента —
/// «не молча», per-op диагностика называет solver); ориентация,
/// недостижимая конусом нутации (defensive-путь — из programmed углов
/// недостижима), — честный носитель.
///
/// Заголовок L1-эмиттера НЕ меняется (machine-координаты осей ≠ TCP —
/// спец-семантика потока задокументирована флагом
/// [`LoweringResult::rtcp_active`] и per-op диагностикой, «не молча»).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RtcpMode {
    /// Дефолт (MachineKinematics::Triv): RTCP-кинематика НЕ задана —
    /// поведение 12.2: ротационный кадр → [`Op::Raw`]-носитель с
    /// подставленными значениями (T4); [`LoweringResult::rtcp_active`]
    /// остаётся `false`.
    Off,
    /// XYZAC table-rotary-tilting профиль (геометрия станка —
    /// [`TrtParams`], ядро kinematics.rs @94cbe11): ходы с полной
    /// TCP-позой опускаются в L1-опы machine-осей через
    /// [`crate::kinematics::xyzac_inverse`] (12.5.3); неполные ходы /
    /// чужие слова — честные носители.
    XyzacTrt { params: TrtParams },
    /// XYZBC table-rotary-tilting профиль (стол B + ротор C, те же
    /// формулы оракула — [`crate::kinematics::xyzbc_inverse`]): семантика
    /// идентична [`RtcpMode::XyzacTrt`] с парой B/C.
    XyzbcTrt { params: TrtParams },
    /// XYZAB table-table TT-2 «cradle/trunnion» (12.5.4; качающийся стол B
    /// и ротор A сверху — Hermle-класс). Ротационные слова A/B = joints
    /// pass-through; линейная TCP опускается через inverse ядра
    /// (`tdr_inverse`, оракул `xyzab_tdr_kins.comp`; dx-инертный квирк
    /// воспроизведён 1:1). Чужое слово: C (топология не моделирует).
    XyzabTdr { params: TdrParams },
    /// Head-head универсальная (ν=90°) + nutating (ν 0–90°) (12.5.4):
    /// A — стол pass-through; B/C — ориентация инструмента через
    /// angle-solver + [`crate::kinematics_trsrn::trsrn_tcp_inverse`]
    /// (см. «Семантика ротационных слов» выше; оракул
    /// `xyzacb_trsrn.comp` + независимый python kins-twp).
    Trsrn { params: TrsrnParams },
    /// Head-table intersecting, DMU 50-класс (12.5.4): ротационные слова
    /// A/B/C = joints pass-through (A — пассивный джоинт оракула),
    /// линейная TCP → [`crate::kinematics_maxkins::maxkins_inverse`];
    /// U/V/W joints — home 0 (не адресуемы в L2-AST); оракул `maxkins.c`.
    Maxkins { params: MaxkinsParams },
    /// Gantry/bridge mill XYZBCW (12.5.4): B (наклон) / C (азимут) =
    /// joints pass-through, W — виртуальная ось вдоль инструмента, joint
    /// держится home 0 (W-слова в L2-AST §2.4 нет — 6 букв X..C);
    /// линейная TCP → [`crate::kinematics_5axis::fiveaxis_inverse`];
    /// оракул `5axiskins.c`.
    FiveAxis { params: FiveAxisParams },
}

impl Default for RtcpMode {
    fn default() -> Self {
        Self::Off
    }
}

// ===========================================================================
// trt-семейство (12.5.3): узкое представление XyzacTrt/XyzbcTrt для
// label-трекинга и TCP→joints lowering 12.5.3 (exec_trt_move/trt_inverse).
// Нелинейные топологии (12.5.4) идут через kins_inverse — см. exec_move.
// ===========================================================================

/// Семейство table-rotary-tilting внутри [`RtcpMode`] (12.5.3-путь).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrtFamily {
    /// XYZAC (A-наклон стола + C-ротор).
    Xyzac,
    /// XYZBC (B-наклон стола + C-ротор).
    Xyzbc,
}

/// Консолидированный trt-профиль: [`RtcpMode::XyzacTrt`]/
/// [`RtcpMode::XyzbcTrt`] → плоское представление с буквами осей и
/// inverse-функцией для 12.5.3-lowering.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrtProfile {
    pub family: TrtFamily,
    pub params: TrtParams,
}

impl TrtProfile {
    /// Some — только для trt-семейства ([`RtcpMode::XyzacTrt`]/[`RtcpMode::XyzbcTrt`]);
    /// нелинейные топологии (12.5.4) и Off — None (их lowering — kins_inverse).
    pub fn of(mode: &RtcpMode) -> Option<Self> {
        match mode {
            RtcpMode::XyzacTrt { params } => Some(Self {
                family: TrtFamily::Xyzac,
                params: *params,
            }),
            RtcpMode::XyzbcTrt { params } => Some(Self {
                family: TrtFamily::Xyzbc,
                params: *params,
            }),
            _ => None,
        }
    }

    /// Имя профиля (диагностика «не молча»).
    pub fn name(&self) -> &'static str {
        match self.family {
            TrtFamily::Xyzac => "XyzacTrt",
            TrtFamily::Xyzbc => "XyzbcTrt",
        }
    }

    /// ЧУЖОЕ ротационное слово (буква вне топологии): Xyzac — B, Xyzbc — A.
    pub fn foreign_letter(&self) -> char {
        match self.family {
            TrtFamily::Xyzac => 'B',
            TrtFamily::Xyzbc => 'A',
        }
    }

    /// СТОЛОВОЕ ротационное слово (наклонная ось): Xyzac — A, Xyzbc — B.
    pub fn table_letter(&self) -> char {
        match self.family {
            TrtFamily::Xyzac => 'A',
            TrtFamily::Xyzbc => 'B',
        }
    }

    /// Имя inverse-функции ядра (диагностика).
    pub fn inverse_fn(&self) -> &'static str {
        match self.family {
            TrtFamily::Xyzac => "xyzac_inverse",
            TrtFamily::Xyzbc => "xyzbc_inverse",
        }
    }

    /// Обновление трекинга углов словами кадра (модальность ротационных
    /// осей trt-семейства — кортеж «стол, ротор» — см. [`RtcpMode::update_rot`]).
    pub fn update_rot(&self, cur: [f64; 3], rp: &ResolvedPose) -> [f64; 3] {
        let w = |word: Option<f64>, keep: f64| word.unwrap_or(keep);
        match self.family {
            TrtFamily::Xyzac => [w(rp.a, cur[0]), cur[1], w(rp.c, cur[2])],
            TrtFamily::Xyzbc => [cur[0], w(rp.b, cur[1]), w(rp.c, cur[2])],
        }
    }
}

// ===========================================================================
// kinematics-профиль lowering (12.5.3/12.5.4): диспетчеризация топологии
// MachineKinematics (pass-through vs solver — см. док [`RtcpMode`])
// ===========================================================================

impl RtcpMode {
    /// Активный kinematics-профиль; [`RtcpMode::Off`] → None.
    /// (allow: B2-диагностики non-trt lowering 12.5.4 — при разморозке
    /// AST/расширении lowering методы возвращаются в использование;
    /// НЕ удалять — документируют семантику топологий.)
    #[allow(dead_code)]
    fn kins(self) -> Option<Self> {
        match self {
            Self::Off => None,
            m => Some(m),
        }
    }

    /// Имя профиля для диагностик («не молча»).
    fn name(self) -> &'static str {
        match self {
            Self::XyzacTrt { .. } => "XyzacTrt",
            Self::XyzbcTrt { .. } => "XyzbcTrt",
            Self::XyzabTdr { .. } => "XyzabTdr",
            Self::Trsrn { .. } => "Trsrn",
            Self::Maxkins { .. } => "Maxkins",
            Self::FiveAxis { .. } => "FiveAxis",
            Self::Off => "Off",
        }
    }

    /// Имя inverse-пути ядра для диагностик (solver у trsrn — из двух
    /// функций: решатель углов + TCP-inverse).
    fn inverse_fn(self) -> &'static str {
        match self {
            Self::XyzacTrt { .. } => "xyzac_inverse",
            Self::XyzbcTrt { .. } => "xyzbc_inverse",
            Self::XyzabTdr { .. } => "tdr_inverse",
            Self::Trsrn { .. } => "trsrn_angles_from_tool_axis + trsrn_tcp_inverse",
            Self::Maxkins { .. } => "maxkins_inverse",
            Self::FiveAxis { .. } => "fiveaxis_inverse",
            Self::Off => "",
        }
    }

    /// Фаза, в которой топология вошла в lowering (диагностика «не молча»).
    /// (allow: B2-диагностики 12.5.4 — см. kins.)
    #[allow(dead_code)]
    fn phase(self) -> &'static str {
        match self {
            Self::XyzacTrt { .. } | Self::XyzbcTrt { .. } => "12.5.3",
            _ => "12.5.4",
        }
    }

    /// Оракул формул (для per-op диагностики).
    fn oracle(self) -> &'static str {
        match self {
            Self::XyzacTrt { .. } | Self::XyzbcTrt { .. } => "trtfuncs",
            Self::XyzabTdr { .. } => "xyzab_tdr_kins.comp",
            Self::Trsrn { .. } => "xyzacb_trsrn.comp + kins-twp",
            Self::Maxkins { .. } => "maxkins.c",
            Self::FiveAxis { .. } => "5axiskins.c",
            Self::Off => "",
        }
    }

    /// Семантика ротационных слов для per-op диагностики (фраза про
    /// pass-through/solver — см. таблицу в док [`RtcpMode`]).
    /// (allow: B2-диагностики 12.5.4 — см. kins.)
    #[allow(dead_code)]
    fn rot_semantics(self) -> &'static str {
        match self {
            Self::XyzacTrt { .. } => "rotary words A/C are joint pass-through",
            Self::XyzbcTrt { .. } => "rotary words B/C are joint pass-through",
            Self::XyzabTdr { .. } => "rotary words A/B are joint pass-through",
            Self::Trsrn { .. } => {
                "table A is joint pass-through; head orientation words B/C are resolved by the \
                 angle solver (main branches C=θ1∈[−90,90], B=θ2∈[0,180])"
            }
            Self::Maxkins { .. } => {
                "rotary words A/B/C are joint pass-through (A passive oracle joint; U/V/W joints \
                 held at home 0 — not addressable in L2 AST)"
            }
            Self::FiveAxis { .. } => {
                "rotary words B/C are joint pass-through; W joint held at home 0 (W word not \
                 addressable in L2 AST)"
            }
            Self::Off => "",
        }
    }

    /// ЧУЖОЕ ротационное слово кадра (буква вне топологии профиля) —
    /// `Some(буква)` → честный носитель: профиль не моделирует ось.
    /// Trsrn/Maxkins чужих букв не имеют (A/B/C — свои; у Maxkins A —
    /// пассивный pass-through джоинт оракула).
    /// (allow: B2-диагностики 12.5.4 — см. kins.)
    #[allow(dead_code)]
    fn foreign_word(self, rp: &ResolvedPose) -> Option<char> {
        match self {
            Self::XyzacTrt { .. } => rp.b.map(|_| 'B'),
            Self::XyzbcTrt { .. } => rp.a.map(|_| 'A'),
            Self::XyzabTdr { .. } => rp.c.map(|_| 'C'),
            Self::FiveAxis { .. } => rp.a.map(|_| 'A'),
            _ => None,
        }
    }

    /// Обновить трекинг углов словами кадра (модальность ротационных осей:
    /// слово Some → новое значение; None → прежний угол держится; home-
    /// допущение [0,0,0] — см. [`L2Evaluator::rot`]).
    ///
    /// Трекается СОСТОЯНИЕ ПРОГРАММНЫХ УГЛОВ (для Trsrn — программная
    /// ориентация: joints решаются из неё на каждом ходе —
    /// [`Self::kins_inverse`]; для остальных программные углы = joints).
    fn update_rot(self, cur: [f64; 3], rp: &ResolvedPose) -> [f64; 3] {
        let w = |word: Option<f64>, keep: f64| word.unwrap_or(keep);
        match self {
            Self::XyzacTrt { .. } => [w(rp.a, cur[0]), cur[1], w(rp.c, cur[2])],
            Self::XyzbcTrt { .. } => [cur[0], w(rp.b, cur[1]), w(rp.c, cur[2])],
            Self::XyzabTdr { .. } => [w(rp.a, cur[0]), w(rp.b, cur[1]), cur[2]],
            Self::Trsrn { .. } | Self::Maxkins { .. } => {
                [w(rp.a, cur[0]), w(rp.b, cur[1]), w(rp.c, cur[2])]
            }
            Self::FiveAxis { .. } => [cur[0], w(rp.b, cur[1]), w(rp.c, cur[2])],
            Self::Off => cur,
        }
    }

    /// Inverse-кинематика топологии: TCP-поза (базовая СК) + углы
    /// (трекинг [`L2Evaluator::rot`]) → machine-оси X/Y/Z.
    /// `None` — ориентация недостижима (только Trsrn: конус нутации;
    /// defensive-путь — из programmed углов решатель всегда достигаем).
    fn kins_inverse(self, xyz_base: [f64; 3], rot: [f64; 3]) -> Option<[f64; 3]> {
        match self {
            Self::XyzacTrt { params } => {
                let pose = Pose6::new(xyz_base[0], xyz_base[1], xyz_base[2], rot[0], 0.0, rot[2]);
                let j = xyzac_inverse(&params, &pose);
                Some([j.x, j.y, j.z])
            }
            Self::XyzbcTrt { params } => {
                let pose = Pose6::new(xyz_base[0], xyz_base[1], xyz_base[2], 0.0, rot[1], rot[2]);
                let j = xyzbc_inverse(&params, &pose);
                Some([j.x, j.y, j.z])
            }
            Self::XyzabTdr { params } => {
                let pose = Pose6::new(xyz_base[0], xyz_base[1], xyz_base[2], rot[0], rot[1], 0.0);
                let j = tdr_inverse(&params, &pose);
                Some([j.x, j.y, j.z])
            }
            Self::Trsrn { params } => {
                // SOLVER-путь (НЕ pass-through): programmed ориентация
                // (B=secondary, C=primary) → вектор оси инструмента Kz
                // (3-й столбец R = Rz(C)·Rs(ν,B)·Rz(tc=0); tc на ось не
                // влияет) → решатель углов (главные ветви) → machine
                // joints (b=θ2, c=θ1) → TCP-inverse линейной части.
                let r = tool_rotation(params.nut_angle, rot[2], rot[1], 0.0);
                let kz = [r[0][2], r[1][2], r[2][2]];
                let (theta1, theta2) = trsrn_angles_from_tool_axis(params.nut_angle, kz)?;
                let pose = Pose6::new(
                    xyz_base[0],
                    xyz_base[1],
                    xyz_base[2],
                    rot[0],
                    theta2,
                    theta1,
                );
                let j = trsrn_tcp_inverse(&params, &pose);
                Some([j.x, j.y, j.z])
            }
            Self::Maxkins { params } => {
                // Порядок букв оракула maxkins.c «joints 0..8 = X Y Z A B C
                // U V W»: A пассивен (pass-through), B — голова, C — стол;
                // U/V/W-салазки не адресуемы в L2-AST — home 0.
                let pose = PoseMaxkins {
                    x: xyz_base[0],
                    y: xyz_base[1],
                    z: xyz_base[2],
                    a: rot[0],
                    b: rot[1],
                    c: rot[2],
                    u: 0.0,
                    v: 0.0,
                    w: 0.0,
                };
                let j = maxkins_inverse(&params, &pose);
                Some([j.x, j.y, j.z])
            }
            Self::FiveAxis { params } => {
                // XYZBCW: W — виртуальная ось вдоль инструмента; joint
                // home 0 (W-слова в L2-AST §2.4 нет — 6 букв X..C).
                let pose = PoseXyzbcw {
                    x: xyz_base[0],
                    y: xyz_base[1],
                    z: xyz_base[2],
                    b: rot[1],
                    c: rot[2],
                    w: 0.0,
                };
                let j = fiveaxis_inverse(&params, &pose);
                Some([j.x, j.y, j.z])
            }
            Self::Off => None,
        }
    }
}

/// Конфигурация интерпретатора/Pass-1 lowering (этап 12.2; 12.5.2 —
/// RTCP-профиль).
#[derive(Debug, Clone, PartialEq)]
pub struct L2Config {
    /// Общий бюджет шагов вычисления (узлы выражений + операторы).
    /// Дефолт 10000 — T5: бесконечный WHILE → E_LIMIT_EXCEEDED.
    pub max_eval_steps: u64,
    /// Бюджет вычислений условия НА КАЖДУЮ сессию цикла WHILE (T5:
    /// «лимит max_eval_steps на КАЖДЫЙ цикл»). Дефолт 10000.
    pub max_loop_steps: u64,
    /// Глубина стека вызовов G65/M98/M97 (дизайн §5 12.2: до 9).
    /// Превышение → [`L2ExecError::CallDepthExceeded`] (E_LIMIT_EXCEEDED).
    pub max_call_depth: usize,
    /// G51.1/G101 зеркало: false (дефолт) → отказ E_UNSUPPORTED_SEMANTICS;
    /// true → инверсия координат + Cw↔Ccw + Notice (T6). Конфиг-структура
    /// lowering (НЕ MachineProfile — по заданию 12.2).
    pub allow_mirror_bake: bool,
    /// Известная начальная позиция станка (MachineProfile/симулятор-хоум).
    /// None (дефолт) → первый ход с неполными словами не lowered
    /// (E_RUNTIME_STATE-носитель); Some — наследование координат с кадра 1.
    pub initial_pos: Option<[f64; 3]>,
    /// RTCP-профиль ротационных осей (12.5.2/12.5.3/12.5.4):
    /// [`RtcpMode::Off`] (дефолт — MachineKinematics::Triv, поведение
    /// 12.2) либо kinematics-профиль с геометрией станка (шесть
    /// топологий: `XyzacTrt`/`XyzbcTrt`/`XyzabTdr`/`Trsrn`/`Maxkins`/
    /// `FiveAxis`) — тогда ходы с полной TCP-позой опускаются в L1-опы
    /// machine-осей (TCP→joints, inverse-кинематика топологии;
    /// подробности, границы и таблица «pass-through vs solver» —
    /// документация [`RtcpMode`]).
    pub rtcp_mode: RtcpMode,
}

impl Default for L2Config {
    fn default() -> Self {
        Self {
            max_eval_steps: 10_000,
            max_loop_steps: 10_000,
            max_call_depth: 9,
            allow_mirror_bake: false,
            initial_pos: None,
            rtcp_mode: RtcpMode::Off,
        }
    }
}

/// Ошибка исполнения/Pass-1 lowering L2. Обёртка над [`L2Error`] + уровневые
/// коды 12.2 (eval.rs заморожен волной 12.1/12.3 — новые коды здесь,
/// аддитивно).
#[derive(Debug, Clone, PartialEq)]
pub enum L2ExecError {
    /// Ошибка вычисления/семантики #0 (`crate::eval`): деление на Null,
    /// LN[#0], исчерпание общего бюджета (`MaxEvalSteps` → T5) и прочее
    /// рантайм-отклонение → классы E_INVALID_INPUT / E_LIMIT_EXCEEDED /
    /// E_RUNTIME_STATE.
    Eval(L2Error),
    /// Глубина стека вызовов G65/M98/M97 превысила лимит (9, дизайн §5
    /// 12.2) → класс E_LIMIT_EXCEEDED.
    CallDepthExceeded { limit: usize },
    /// Бюджет условия одной сессии WHILE исчерпан (T5 per-loop) →
    /// класс E_LIMIT_EXCEEDED.
    LoopLimitExceeded { limit: u64 },
    /// Семантика не представима в L1 (§3): зеркало G51.1/G101 без флага,
    /// носители RTCP/TWP → класс E_UNSUPPORTED_SEMANTICS.
    UnsupportedSemantics(String),
    /// Чтение таблицы WCS ([`SysVar::WcsOffset`]) требует таблицы станка
    /// (§3 семейство состояний станка) → класс E_REQUIRES_MACHINE_TABLE.
    MachineTableRequired(String),
}

impl L2ExecError {
    /// Класс диагностики SPEC §8.
    pub fn error_code(&self) -> ErrorCode {
        match self {
            Self::Eval(e) => e.error_code(),
            Self::CallDepthExceeded { .. } | Self::LoopLimitExceeded { .. } => {
                ErrorCode::LimitExceeded
            }
            Self::UnsupportedSemantics(_) => ErrorCode::UnsupportedSemantics,
            Self::MachineTableRequired(_) => ErrorCode::RequiresMachineTable,
        }
    }

    /// Человекочитаемое сообщение (для Diagnostic.message).
    pub fn message(&self) -> String {
        match self {
            Self::Eval(e) => e.message(),
            Self::CallDepthExceeded { limit } => {
                format!("subprogram call stack depth exceeded limit {limit} (design §5 12.2)")
            }
            Self::LoopLimitExceeded { limit } => {
                format!("WHILE loop step budget exhausted (per-loop limit {limit})")
            }
            Self::UnsupportedSemantics(m) | Self::MachineTableRequired(m) => m.clone(),
        }
    }
}

impl From<L2Error> for L2ExecError {
    fn from(e: L2Error) -> Self {
        Self::Eval(e)
    }
}

// ===========================================================================
// Снапшот модальностей (SysVar::ModalGroup — статически разрешим из state)
// ===========================================================================

/// Снапшот модальных групп (§2.2 `SysVar::ModalGroup(u8)`, Fanuc #4001..):
/// power-on дефолты Fanuc mill для МОДЕЛИРУЕМОГО подмножества (честно:
/// только то, что lowering реально трекает); обновляется lowering'ом.
/// Немоделируемая группа при чтении → останов E_RUNTIME_STATE (§3).
///
/// | группа | дефолт | смысл               | обновление |
/// |--------|--------|---------------------|------------|
/// | 1      | 0      | motion G0/G1/G2/G3  | lowering ходов |
/// | 2      | 17     | плоскость G17/18/19 | НЕ трекается (нет узла AST — контракт-пробел, шапка) |
/// | 3      | 90     | G90/G91             | не трекается |
/// | 4      | 94     | G94/G95 feed        | не трекается |
/// | 6      | 21     | units G20/G21       | не трекается |
/// | 7      | 40     | G40/41/42           | не трекается |
/// | 8      | 49     | G43/49 length       | не трекается |
/// | 9      | 80     | canned cancel       | не трекается |
/// | 14     | 54     | WCS G54..59         | не трекается |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modals {
    groups: BTreeMap<u8, u32>,
}

impl Default for Modals {
    fn default() -> Self {
        Self::new()
    }
}

impl Modals {
    /// Power-on снапшот Fanuc mill (моделируемое подмножество).
    pub fn new() -> Self {
        let groups: BTreeMap<u8, u32> = [
            (1u8, 0u32), // G0
            (2, 17),     // G17
            (3, 90),     // G90
            (4, 94),     // G94
            (6, 21),     // G21 (mm)
            (7, 40),     // G40
            (8, 49),     // G49
            (9, 80),     // G80
            (14, 54),    // G54
        ]
        .into_iter()
        .collect();
        Self { groups }
    }

    /// Значение группы (Fanuc-код: #4001 → 0/1/2/3, #4002 → 17/18/19, …);
    /// немоделируемая → None (читатель даёт останов E_RUNTIME_STATE).
    pub fn value(&self, g: u8) -> Option<u32> {
        self.groups.get(&g).copied()
    }

    /// Активная плоскость (группа 2).
    pub fn plane(&self) -> Plane {
        match self.value(2) {
            Some(18) => Plane::ZX,
            Some(19) => Plane::YZ,
            _ => Plane::XY,
        }
    }

    /// Обновить группу motion (lowering хода: G0→0, G1→1, G2→2, G3→3).
    fn set_motion(&mut self, code: u32) {
        self.groups.insert(1, code);
    }
}

// ===========================================================================
// Носители RAW (маркеры 12.1 — совместимы; семантика 12.2 в шапке)
// ===========================================================================

/// Маркер носителя RAW: что за L2-конструкция не lowered в Pass-1 и зачем.
/// (12.2: `ControlFlow`/`Parametric` — ЛЕГАТЫ 12.1: с полным CFG/D0 носители
/// этих классов больше не строятся; варианты оставлены для совместимости API.)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawMarker {
    /// ЛЕГАТ 12.1 (D0 полный: параметрика разрешается). Не конструируется.
    Parametric,
    /// Ротационная ось A/B/C в кадре (§3 строка 5, T4-payload).
    RotaryAxis,
    /// Дуга не lowered (геометрия не задана/позиция неизвестна).
    ArcUnlowered,
    /// Ход не lowered: слово отсутствует/Null при неизвестной текущей
    /// позиции (адресная семантика 2.1 — координата наследуется).
    IncompletePose,
    /// Трансформация не lowered (TWP §3 строка 6) — тело подавлено.
    Transform,
    /// ЛЕГАТ 12.1 (D2: IfGoto исполняется CFG). Не конструируется.
    ControlFlow,
    /// Модальный вызов G66 — каркас (регистрация; триггер после motion).
    SubCall,
    /// RTCP-режим (G43.4/G43.5/G49).
    Rtcp,
}

impl RawMarker {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Parametric => "parametric-move",
            Self::RotaryAxis => "rotary-axis",
            Self::ArcUnlowered => "arc-geometry",
            Self::IncompletePose => "incomplete-pose",
            Self::Transform => "transform",
            Self::ControlFlow => "if-goto",
            Self::SubCall => "modal-subcall",
            Self::Rtcp => "rtcp",
        }
    }
}

/// Носитель не-lowered L2-конструкции: маркер + `Op::Raw` (lossless
/// payload — hex ASCII-кадра с подставленными значениями [`format_frame`],
/// причина, честный код, спан оператора из AST).
#[derive(Debug, Clone, PartialEq)]
pub struct RawTailItem {
    pub marker: RawMarker,
    /// Всегда `Op::Raw` — носитель.
    pub op: Op,
}

/// Результат Pass-1 (lowering L2 → L1, §3).
#[derive(Debug, Clone, Default)]
pub struct LoweringResult {
    /// Успешно lowered L1-опы (в т.ч. passthrough `L2Stmt::L1Op`).
    pub l1_ops: Vec<Op>,
    /// Хвост носителей RAW (маркер + Op::Raw) — порядок исполнения.
    pub raw_tail: Vec<RawTailItem>,
    /// Диагностики пасса (Stage::Canonicalize; носители + Notice'и —
    /// например, запечённое зеркало T6). Остановы (Err) отражены в
    /// возврате [`L2Evaluator::run`], программа остаётся в L2 (§3).
    pub diagnostics: Vec<Diagnostic>,
    /// RTCP-профиль участвовал в lowering (12.5.2/12.5.3): `true` ⇔ под
    /// trt-профилем ([`RtcpMode::XyzacTrt`]/[`RtcpMode::XyzbcTrt`])
    /// построен ХОТЯ БЫ ОДИН trt-элемент — joints-оп machine-осей
    /// (TCP→joints через inverse-кинематику) или trt-носитель (неполная
    /// поза / чужое ротационное слово / дуга). Смысл для потребителя:
    /// L1-опы в [`Self::l1_ops`] НЕсут machine-координаты осей X/Y/Z, а
    /// НЕ TCP — заголовок эмиттера (`G1 X.. Y.. Z..`) при этом прежний
    /// (спец-семантика потока, «не молча»: per-op диагностика называет
    /// профиль). `false` при [`RtcpMode::Off`] и когда trt-путь не
    /// задействован ни разу.
    pub rtcp_active: bool,
}

impl LoweringResult {
    /// Носитель RAW + диагностика. `payload` — ТЕКСТ кадра с подставленными
    /// значениями (§3/T4; hex-кодирование в [`raw_frame_op`]); None →
    /// легаси debug-дамп оператора.
    fn push_raw(
        &mut self,
        marker: RawMarker,
        code: ErrorCode,
        payload: Option<String>,
        stmt: &L2Stmt,
        detail: &str,
    ) {
        let src = stmt
            .src()
            .cloned()
            .unwrap_or_else(crate::ast::placeholder_span);
        let reason = format!("L2 pass-1 [{}]: {detail}", marker.as_str());
        let op = match payload {
            Some(text) => raw_frame_op(&text, reason.clone(), code, src),
            None => Op::Raw {
                payload_hex: payload_hex_of_debug(stmt),
                reason: reason.clone(),
                code,
                src,
            },
        };
        self.raw_tail.push(RawTailItem { marker, op });
        self.diagnostics.push(Diagnostic {
            code,
            stage: Stage::Canonicalize,
            message: reason,
            src: stmt.src().cloned(),
            options: Vec::new(),
            limit: None,
        });
    }
}

/// Легаси-fallback (12.1): hex детерминированного debug-дампа оператора.
fn payload_hex_of_debug(stmt: &L2Stmt) -> String {
    let dump = format!("{stmt:?}");
    let mut hex = String::with_capacity(dump.len() * 2);
    for b in dump.as_bytes() {
        hex.push_str(&format!("{b:02X}"));
    }
    hex
}

/// Состояние по умолчанию для синтезированных L1-опов (12.1-решение:
/// spindle/coolant/tool — как в грамматике D3 до первого слова; S-слово
/// хода не пишется — направление Cw/Ccw требует M3/M4-контекста, шапка).
fn l1_default_state(feed: Option<Num>, feed_in_frame: bool) -> State {
    State {
        feed,
        spindle: eid_core::ast::Spindle::Off,
        coolant: eid_core::ast::Coolant::default(),
        tool: None,
        frame: eid_core::ast::Frame::Wcs,
        wcs: eid_core::ast::Wcs::G54,
        // Провенанс «F-слово было в кадре-источнике» (INTERFACE-V0.2 §2.4):
        // ход lowering'а — сам кадр-источник → флаг = наличие F-слова.
        feed_restated: feed_in_frame,
    }
}

/// f64 → Num: single-final-rounding 4dp (SPEC §10.5) + канонический текст
/// (детерминированный, `-0.0000` → `0.0000`).
fn mk_num4(v: f64) -> Num {
    let mut q = (v * 10_000.0).round() / 10_000.0;
    if q == 0.0 {
        q = 0.0; // канонизация -0.0 → +0.0 (бит-паттерн)
    }
    Num::new(eid_core::emitter::fmt_num(q), q)
}

/// Наследование трёх осей: слово Some → значение; None → текущая позиция
/// (модальная ось); позиция тоже неизвестна → None.
fn inherit_axes(words: [Option<f64>; 3], cur: Option<[f64; 3]>) -> Option<[f64; 3]> {
    match (
        words[0].or(cur.map(|p| p[0])),
        words[1].or(cur.map(|p| p[1])),
        words[2].or(cur.map(|p| p[2])),
    ) {
        (Some(x), Some(y), Some(z)) => Some([x, y, z]),
        _ => None,
    }
}

fn pt3(p: [f64; 3]) -> Point3 {
    Point3 {
        x: mk_num4(p[0]),
        y: mk_num4(p[1]),
        z: mk_num4(p[2]),
    }
}

// ===========================================================================
// pose_resolve (D0): Null-компонента → слово отсутствует (таблица 2.1)
// ===========================================================================

/// Поза с разрешёнными значениями слов: None — слова нет или оно Null
/// (игнорируется), Some(v) — конкретное значение.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ResolvedPose {
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub z: Option<f64>,
    pub a: Option<f64>,
    pub b: Option<f64>,
    pub c: Option<f64>,
}

/// Вычислить все слова позы в ЧИСТОМ окружении (без снапшота модальностей):
/// слово с Null игнорируется — «как будто не написано» (таблица 2.1);
/// Sys-чтение → E_RUNTIME_STATE-останов (§3). Для lowering с снапшотом
/// модальностей интерпретатор разрешает слова через `eval_resolved`
/// (см. [`L2Evaluator`]).
pub fn pose_resolve(pose: &ExprPose, env: &mut L2Env) -> Result<ResolvedPose, L2Error> {
    let mut word = |e: &Option<Expr>| -> Result<Option<f64>, L2Error> {
        match e {
            None => Ok(None),
            Some(expr) => Ok(eval_expr(expr, env)?.resolve_word()),
        }
    };
    Ok(ResolvedPose {
        x: word(&pose.x)?,
        y: word(&pose.y)?,
        z: word(&pose.z)?,
        a: word(&pose.a)?,
        b: word(&pose.b)?,
        c: word(&pose.c)?,
    })
}

// ===========================================================================
// L2Evaluator (D0–D3 + полный Pass-1)
// ===========================================================================

/// Сигнал потока управления внутри одного тела (body-scope): Jump возвращает
/// только [`L2Stmt::IfGoto`] и поглощается [`L2Evaluator::exec_block`];
/// Return пузырится до точки вызова подпрограммы.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Flow {
    Normal,
    Jump(usize),
    Return,
}

/// Интерпретатор L2, уровни D0–D3, с полным Pass-1 lowering (§3).
#[derive(Debug, Clone)]
pub struct L2Evaluator {
    config: L2Config,
    env: L2Env,
    /// Программная позиция (значения адресных слов; наследование осей).
    /// Базовая СК = `xform.apply_point(pos_prog)` (трансформации §3).
    pos_prog: Option<[f64; 3]>,
    /// Трекинг ротационных углов (12.5.3/12.5.4, под kinematics-профилем):
    /// `[A, B, C]` программных углов (используются компоненты СВОИХ букв
    /// топологии; Trsrn хранит программную ориентацию — joints решаются
    /// из неё на каждом ходе, см. [`RtcpMode::kins_inverse`]).
    /// Home-допущение [0, 0, 0] — тот же класс допущений, что
    /// [`L2Config::initial_pos`] для линейных осей (rs274-сим: home 0,
    /// сверки trace-тестов); ротационные слова кадров обновляют
    /// компоненты по отдельности (модальность ротационных осей: угол
    /// держится до следующего слова).
    rot: [f64; 3],
    /// Позиция MACHINE-осей X/Y/Z (12.5.3, только под trt-профилем):
    /// end последнего trt-опа. Start-point следующего joints-опа. None —
    /// machine-позиция неизвестна (до первого trt-опа; ПОСЛЕ носителя
    /// движения — станок сместился по кадру, который Pass-1 не опустил,
    /// — сбрасывается в None: start честно неизвестен). В [`RtcpMode::Off`]
    /// не используется.
    pos_joints: Option<[f64; 3]>,
    /// Снапшот модальностей (SysVar::ModalGroup — статически разрешим).
    modals: Modals,
    /// Активная аффинная трансформация (композиция стека G68/G51/G51.1).
    xform: Affine3,
    xform_stack: Vec<Affine3>,
    /// Модальный вызов G66 (каркас): цель + ЗАПИСАННЫЕ значения аргументов
    /// (индексы локальных #1..#26 + значения; вычислены в момент
    /// регистрации, контекст вызывающего).
    modal_call: Option<(SubTarget, Vec<(u32, Val)>)>,
    out: LoweringResult,
    /// Per-body таблицы меток (D2): адрес тела → N-метка → индекс.
    body_labels: HashMap<usize, BTreeMap<u32, usize>>,
    main_labels: BTreeMap<u32, usize>,
}

impl L2Evaluator {
    /// Новый интерпретатор с конфигом (бюджет навязывается окружению).
    pub fn new(config: L2Config) -> Self {
        let mut env = L2Env::new();
        env.set_budget(config.max_eval_steps);
        Self {
            config,
            env,
            pos_prog: None,
            rot: [0.0; 3],
            pos_joints: None,
            modals: Modals::new(),
            xform: Affine3::identity(),
            xform_stack: Vec::new(),
            modal_call: None,
            out: LoweringResult::default(),
            body_labels: HashMap::new(),
            main_labels: BTreeMap::new(),
        }
    }

    /// Окружение (переменные после исполнения — для тестов/диагностики).
    pub fn env(&self) -> &L2Env {
        &self.env
    }

    /// Конфиг.
    pub fn config(&self) -> &L2Config {
        &self.config
    }

    /// Исполнить программу (D0–D3) и собрать Pass-1 (§3). Любая ошибка
    /// ([`L2ExecError`]) ОСТАНАВЛИВАЕТ исполнение — программа остаётся в L2
    /// (§3 последняя строка матрицы: G31/позиции/таблицы — честный останов).
    ///
    /// Состояние интерпретатора СБРАСЫВАЕТСЯ на каждый прогон
    /// (переменные/позиция/модальности/трансформации) — прогон
    /// детерминирован и повторяем.
    pub fn run(&mut self, program: &L2Program) -> Result<LoweringResult, L2ExecError> {
        let mut env = L2Env::new();
        env.set_budget(self.config.max_eval_steps);
        self.env = env;
        self.pos_prog = self.config.initial_pos;
        self.rot = [0.0; 3];
        self.pos_joints = None;
        self.modals = Modals::new();
        self.xform = Affine3::identity();
        self.xform_stack.clear();
        self.modal_call = None;
        self.out = LoweringResult::default();
        // D2: предвычисление per-body таблиц меток (главный блок + subs +
        // тела While/WithTransform на любой глубине).
        self.body_labels.clear();
        precompute_labels(&program.stmts, &mut self.body_labels);
        for sub in &program.subs {
            precompute_labels(&sub.body, &mut self.body_labels);
        }
        self.main_labels = resolve_labels(program);
        let labels = self.main_labels.clone();
        match self.exec_block(program, &program.stmts, &labels)? {
            Flow::Normal => {}
            // Защита: Return доходит до корня только при ошибке баланса
            // кадров (сам Return проверяет глубину) — сообщаем честно.
            Flow::Return => {
                return Err(L2ExecError::Eval(L2Error::RuntimeError(
                    "M99/RETURN escaped to program root (frame imbalance)".to_string(),
                )))
            }
            Flow::Jump(_) => unreachable!("Jump must be absorbed by exec_block"),
        }
        Ok(std::mem::take(&mut self.out))
    }

    // ----- служебные -------------------------------------------------------

    /// Вычислить выражение с снапшотом модальностей: SysVar::ModalGroup
    /// разрешается статически из state (§3); WcsOffset →
    /// E_REQUIRES_MACHINE_TABLE; позиции → E_RUNTIME_STATE-останов.
    fn eval_resolved(&mut self, e: &Expr) -> Result<Val, L2ExecError> {
        let resolved = self.resolve_sys(e)?;
        eval_expr(&resolved, &mut self.env).map_err(L2ExecError::Eval)
    }

    /// Переписать Sys-ссылки в выражении (§3 SysVar-маппинг):
    /// ModalGroup → Const(значение снапшота); WcsOffset → останов
    /// E_REQUIRES_MACHINE_TABLE; позиции/вендорские → останов
    /// E_RUNTIME_STATE (программа остаётся в L2 — §3 последняя строка).
    fn resolve_sys(&self, e: &Expr) -> Result<Expr, L2ExecError> {
        match e {
            Expr::Const(_) | Expr::Null => Ok(e.clone()),
            Expr::Var(VarRef::Sys(sv)) => match sv {
                SysVar::ModalGroup(g) => match self.modals.value(*g) {
                    Some(v) => Ok(Expr::num(v as f64)),
                    None => Err(L2ExecError::Eval(L2Error::RuntimeError(format!(
                        "modal group #{g} is not modeled in the L2 modal snapshot — E_RUNTIME_STATE stop (§3)"
                    )))),
                },
                SysVar::WcsOffset { p, axis } => Err(L2ExecError::MachineTableRequired(format!(
                    "WCS offset table read #WCS[{}.{}] requires machine table — E_REQUIRES_MACHINE_TABLE (§3)",
                    p,
                    axis.letter()
                ))),
                sv_pos @ (SysVar::AbsMachinePos(_)
                | SysVar::WorkPos(_)
                | SysVar::SkipPos(_)
                | SysVar::VendorSpecific(_)) => Err(L2ExecError::Eval(L2Error::RuntimeError(
                    format!(
                        "system variable {} requires machine runtime state (unknown in Pass-1) — E_RUNTIME_STATE stop; program remains in L2 (§3)",
                        sys_var_display(sv_pos)
                    ),
                ))),
            },
            Expr::Var(_) => Ok(e.clone()),
            Expr::Unary(op, inner) => Ok(Expr::Unary(*op, Box::new(self.resolve_sys(inner)?))),
            Expr::Binary(op, a, b) => Ok(Expr::Binary(
                *op,
                Box::new(self.resolve_sys(a)?),
                Box::new(self.resolve_sys(b)?),
            )),
        }
    }

    /// Разрешить слова позы (eval_resolved: Null → None = слово игнорируется).
    fn pose_resolved(&mut self, pose: &ExprPose) -> Result<ResolvedPose, L2ExecError> {
        let mut word = |e: &Option<Expr>| -> Result<Option<f64>, L2ExecError> {
            match e {
                None => Ok(None),
                Some(expr) => Ok(self.eval_resolved(expr)?.resolve_word()),
            }
        };
        Ok(ResolvedPose {
            x: word(&pose.x)?,
            y: word(&pose.y)?,
            z: word(&pose.z)?,
            a: word(&pose.a)?,
            b: word(&pose.b)?,
            c: word(&pose.c)?,
        })
    }

    /// Таблица меток тела (предвычислена в [`L2Evaluator::run`]).
    fn labels_of(&self, stmts: &[L2Stmt]) -> BTreeMap<u32, usize> {
        self.body_labels
            .get(&(stmts.as_ptr() as usize))
            .cloned()
            .unwrap_or_default()
    }

    /// Базовая СК-позиция как StartPoint для lowered хода.
    fn pos_start(&self) -> StartPoint {
        match self.pos_prog {
            Some(p) => StartPoint::Known(pt3(self.xform.apply_point(p))),
            None => StartPoint::Unknown,
        }
    }

    // ----- исполнение ------------------------------------------------------

    /// Блок операторов (последовательный CFG; Jump поглощается, Return
    /// пузырится).
    fn exec_block(
        &mut self,
        program: &L2Program,
        stmts: &[L2Stmt],
        labels: &BTreeMap<u32, usize>,
    ) -> Result<Flow, L2ExecError> {
        let mut i = 0usize;
        while i < stmts.len() {
            match self.exec_stmt(program, &stmts[i], labels)? {
                Flow::Normal => i += 1,
                Flow::Jump(idx) => i = idx,
                Flow::Return => return Ok(Flow::Return),
            }
        }
        Ok(Flow::Normal)
    }

    /// Один оператор.
    fn exec_stmt(
        &mut self,
        program: &L2Program,
        stmt: &L2Stmt,
        labels: &BTreeMap<u32, usize>,
    ) -> Result<Flow, L2ExecError> {
        self.env.tick()?;
        match stmt {
            // ---------- D0 ----------
            L2Stmt::Assign { target, value, .. } => {
                let v = self.eval_resolved(value)?;
                // Присваивание копирует Null (таблица 2.1: `#100 = #0` —
                // переменная становится Val::Null, НЕ 0.0).
                write_var(target, v, &mut self.env)?;
                Ok(Flow::Normal)
            }
            L2Stmt::Move { .. } => self.exec_move(program, stmt),
            L2Stmt::L1Op(op) => {
                self.out.l1_ops.push(op.clone());
                Ok(Flow::Normal)
            }
            L2Stmt::Label { .. } => Ok(Flow::Normal), // no-op (позиция GOTO)

            // ---------- D1 ----------
            L2Stmt::IfThen {
                cond, then_stmt, ..
            } => {
                // cond: ≠0 → true; IF [#0] — Null как 0.0 → false.
                // D3: константный cond разрешается здесь же — мёртвая ветка
                // (и Sys-чтения в ней) не вычисляется вовсе.
                let c = self.eval_resolved(cond)?;
                if c.truthy() {
                    // then_stmt живёт в ОБОЛОЧКЕ текущего тела (GOTO внутри
                    // THEN — переход в пространстве меток этого тела).
                    self.exec_stmt(program, then_stmt, labels)
                } else {
                    Ok(Flow::Normal)
                }
            }
            L2Stmt::Return { .. } => {
                if self.env.frame_depth() > 1 {
                    Ok(Flow::Return) // M99 → pop кадра делает call-site
                } else {
                    Err(L2ExecError::Eval(L2Error::RuntimeError(
                        "M99/RETURN outside subprogram call (frame stack root)".to_string(),
                    )))
                }
            }

            // ---------- D2 (CFG) ----------
            L2Stmt::IfGoto {
                cond, target_label, ..
            } => {
                let c = self.eval_resolved(cond)?;
                if c.truthy() {
                    match labels.get(target_label) {
                        Some(&idx) => Ok(Flow::Jump(idx)),
                        None => Err(L2ExecError::Eval(L2Error::RuntimeError(format!(
                            "GOTO target N{target_label} not found (label table; contract: parser fills L2Program::labels or emits L2Stmt::Label)"
                        )))),
                    }
                } else {
                    Ok(Flow::Normal)
                }
            }
            L2Stmt::While {
                cond, do_id, body, ..
            } => {
                // DO/END-парность — на парсере; do_id 1..3 проверяется и здесь.
                if !(1..=3).contains(do_id) {
                    return Err(L2ExecError::Eval(L2Error::RuntimeError(format!(
                        "WHILE DO{do_id}: do_id must be 1..=3"
                    ))));
                }
                let body_labels = self.labels_of(body);
                let mut loop_steps = 0u64;
                loop {
                    // Бюджет КАЖДОЙ сессии цикла (T5): cond-вычисления
                    // считаются отдельно от общего бюджета.
                    loop_steps += 1;
                    if loop_steps > self.config.max_loop_steps {
                        return Err(L2ExecError::LoopLimitExceeded {
                            limit: self.config.max_loop_steps,
                        });
                    }
                    self.env.tick()?;
                    let c = self.eval_resolved(cond)?;
                    if !c.truthy() {
                        break;
                    }
                    // Return внутри тела — легальный ранний выход из
                    // подпрограммы: пузырится до call-site.
                    if let Flow::Return = self.exec_block(program, body, &body_labels)? {
                        return Ok(Flow::Return);
                    }
                }
                Ok(Flow::Normal)
            }
            L2Stmt::CallSub { .. } => self.exec_call_sub(program, stmt),

            // ---------- §3: носители/трансформации ----------
            L2Stmt::WithTransform {
                transform, body, ..
            } => self.exec_with_transform(program, transform, body, stmt),
            L2Stmt::RtcpMode {
                enabled,
                tool_length_h,
                ..
            } => {
                // §3 строка 6: RTCP в Pass-1 не опускается → Raw с
                // подставленными значениями (H — u32-номер, не выражение).
                let text = match (enabled, tool_length_h) {
                    (true, Some(h)) => format!("G43.4 H{h}\n"),
                    (true, None) => "G43.4\n".to_string(),
                    (false, _) => "G49\n".to_string(),
                };
                self.out.push_raw(
                    RawMarker::Rtcp,
                    ErrorCode::UnsupportedSemantics,
                    Some(text),
                    stmt,
                    "RTCP kinematics not lowered in Pass 1 (§3)",
                );
                Ok(Flow::Normal)
            }
        }
    }

    /// WithTransform (§2.3/§3 строки 3-4): G68/G51 → аффинная матрица на
    /// тело; G51.1/G101 → дефолт-отказ / bake-флаг; TWP → Raw-носитель с
    /// подавленным телом.
    fn exec_with_transform(
        &mut self,
        program: &L2Program,
        transform: &TransformKind,
        body: &[L2Stmt],
        stmt: &L2Stmt,
    ) -> Result<Flow, L2ExecError> {
        match transform {
            TransformKind::Rotate2D {
                center,
                angle_deg,
                plane,
            } => {
                let c0 = self.eval_resolved(&center[0])?.as_num();
                let c1 = self.eval_resolved(&center[1])?.as_num();
                let ang = self.eval_resolved(angle_deg)?.as_num();
                if !ang.is_finite() {
                    return Err(L2ExecError::Eval(L2Error::RuntimeError(format!(
                        "G68 angle not finite: {ang}"
                    ))));
                }
                let t = Affine3::rotate_plane(*plane, [c0, c1], ang);
                self.push_transform(t);
                let body_labels = self.labels_of(body);
                let res = self.exec_block(program, body, &body_labels);
                self.pop_transform();
                res
            }
            TransformKind::Scale { center, factor } => {
                let c = [
                    self.eval_resolved(&center[0])?.as_num(),
                    self.eval_resolved(&center[1])?.as_num(),
                    self.eval_resolved(&center[2])?.as_num(),
                ];
                let f = self.eval_resolved(factor)?.as_num();
                if !f.is_finite() || f == 0.0 {
                    return Err(L2ExecError::Eval(L2Error::RuntimeError(format!(
                        "G51 degenerate scale factor: {f}"
                    ))));
                }
                let t = Affine3::scale_uniform(c, f);
                self.push_transform(t);
                let body_labels = self.labels_of(body);
                let res = self.exec_block(program, body, &body_labels);
                self.pop_transform();
                res
            }
            TransformKind::Mirror { axis, pivot } => {
                // T6 / §3 строка 4: дефолт — отказ (climb↔conventional
                // семантика не представима ходами молча).
                if !self.config.allow_mirror_bake {
                    return Err(L2ExecError::UnsupportedSemantics(
                        "G51.1/G101 mirror changes climb↔conventional — not representable by default (E_UNSUPPORTED_SEMANTICS, T6); set allow_mirror_bake=true to bake".to_string(),
                    ));
                }
                let ax = mirror_axis_index(axis)?;
                let pv = self.eval_resolved(pivot)?.as_num();
                let t = Affine3::mirror_axis(ax, pv);
                self.push_transform(t);
                // Notice: запечённое зеркало (T6) — смена семантики
                // направления фрезерования честно отражена диагностикой.
                self.out.diagnostics.push(Diagnostic {
                    code: ErrorCode::UnsupportedSemantics,
                    stage: Stage::Canonicalize,
                    message: "mirror baked into coordinates (allow_mirror_bake): axis inverted, Cw↔Ccw flipped; climb↔conventional semantics changed (T6)".to_string(),
                    src: stmt.src().cloned(),
                    options: vec!["allow_mirror_bake".to_string()],
                    limit: None,
                });
                let body_labels = self.labels_of(body);
                let res = self.exec_block(program, body, &body_labels);
                self.pop_transform();
                res
            }
            TransformKind::TiltedWorkPlane { origin, euler } => {
                // §3 строка 6: TWP не опускается; тело подавлено (ходы без
                // TWP дали бы неверную геометрию). Raw с подставленными
                // значениями (синтетический порядок слов X Y Z A B C —
                // вендорские синтаксисы G68.2 расходятся; корпусного
                // драйвера нет — ценз B).
                let o = [
                    self.eval_resolved(&origin[0])?.as_num(),
                    self.eval_resolved(&origin[1])?.as_num(),
                    self.eval_resolved(&origin[2])?.as_num(),
                ];
                let e = [
                    self.eval_resolved(&euler[0])?.as_num(),
                    self.eval_resolved(&euler[1])?.as_num(),
                    self.eval_resolved(&euler[2])?.as_num(),
                ];
                let text = format_frame(
                    "G68.2",
                    &[
                        ('X', o[0]),
                        ('Y', o[1]),
                        ('Z', o[2]),
                        ('A', e[0]),
                        ('B', e[1]),
                        ('C', e[2]),
                    ],
                );
                self.out.push_raw(
                    RawMarker::Transform,
                    ErrorCode::UnsupportedSemantics,
                    Some(text),
                    stmt,
                    "tilted work plane (G68.2) not lowered in Pass 1 (§3) — body suppressed",
                );
                Ok(Flow::Normal)
            }
        }
    }

    fn push_transform(&mut self, t: Affine3) {
        let composed = self.xform.compose(&t);
        self.xform_stack.push(self.xform);
        self.xform = composed;
    }

    fn pop_transform(&mut self) {
        self.xform = self.xform_stack.pop().unwrap_or_else(Affine3::identity);
    }

    // ----- Move (§3: строки 1, 5, 7 + дуги) ---------------------------------

    /// Move: разрешение слов, наследование координат, трансформация,
    /// lowering в L1-оп/носитель.
    fn exec_move(&mut self, program: &L2Program, stmt: &L2Stmt) -> Result<Flow, L2ExecError> {
        let (kind, target, feed, spindle) = match stmt {
            L2Stmt::Move {
                kind,
                target,
                feed,
                spindle,
                ..
            } => (kind, target, feed, spindle),
            _ => unreachable!("exec_move called on non-Move"),
        };
        // 1) Разрешение слов (ошибки выражений и Sys-чтений — останов §3).
        //    Даже для носителя: G31/SkipPos при неизвестном — останов
        //    E_RUNTIME_STATE, а не молчаливый RAW.
        let rp = self.pose_resolved(target)?;
        let feed_v = match feed {
            Some(e) => self.eval_resolved(e)?.resolve_word(), // Null-F → слова нет
            None => None,
        };
        // S-слово: не пишется в State (нет M3/M4-контекста в AST §2.4 —
        // решение 12.1, шапка); параметричность/константность S НЕ блокирует
        // lowering (12.2). Значение вычисляется только ради ошибок/Sys-останов.
        if let Some(e) = spindle {
            self.eval_resolved(e)?;
        }

        match kind {
            MoveKind::Rapid | MoveKind::Linear => {
                let gcode = match kind {
                    MoveKind::Rapid => "G0",
                    _ => "G1",
                };
                // 2) trt-профиль (12.5.3): kinematics-схема MachineKinematics
                //    ([`RtcpMode::XyzacTrt`]/[`RtcpMode::XyzbcTrt`]) — ходы с
                //    полной TCP-позой опускаются в L1-опы machine-осей
                //    (TCP→joints); Off — прежние носители T4 (§3 строка 5).
                // 3.5) Нелинейные топологии (12.5.4): XyzabTdr/Trsrn/Maxkins/
                //      FiveAxis — TCP→joints через kins_inverse (полный-TCP
                //      скоуп 12.5.3-семантики: ход опускается в L1-оп
                //      machine-осей; дуги → честный носитель, не аффинно).
                if matches!(
                    self.config.rtcp_mode,
                    RtcpMode::XyzabTdr { .. }
                        | RtcpMode::Trsrn { .. }
                        | RtcpMode::Maxkins { .. }
                        | RtcpMode::FiveAxis { .. }
                ) {
                    // Углы: слова кадра обновляют трекинг (rot_semantics
                    // топологии — pass-through или solver для Trsrn).
                    let rot = self.config.rtcp_mode.update_rot(self.rot, &rp);
                    let end_base = self.xform.apply_point(
                        inherit_axes([rp.x, rp.y, rp.z], self.pos_prog)
                            .unwrap_or_else(|| self.pos_prog.unwrap_or([0.0; 3])),
                    );
                    let Some(joints_end) = self.config.rtcp_mode.kins_inverse(end_base, rot) else {
                        // Trsrn: ориентация недостижима (конус нутации) —
                        // честный носитель вместо молчаливого pass.
                        let mut words = frame_words(&rp);
                        if let Some(f) = feed_v {
                            words.push(('F', f));
                        }
                        let text = format_frame(gcode, &words);
                        self.out.push_raw(
                            RawMarker::RotaryAxis,
                            ErrorCode::UnsupportedSemantics,
                            Some(text),
                            stmt,
                            "orientation unreachable under MachineKinematics profile (12.5.4) — honest carrier, not silent pass",
                        );
                        self.out.rtcp_active = true;
                        self.update_pos_prog(&rp);
                        self.rot = rot;
                        return Ok(Flow::Normal);
                    };
                    let feed_num = feed_v.map(mk_num4);
                    let state = l1_default_state(feed_num, feed_v.is_some());
                    let src = stmt
                        .src()
                        .cloned()
                        .unwrap_or_else(crate::ast::placeholder_span);
                    let start = match self.pos_joints {
                        Some(j) => StartPoint::Known(pt3(j)),
                        None => StartPoint::Unknown,
                    };
                    let op = if *kind == MoveKind::Rapid {
                        Op::Rapid {
                            start,
                            end: pt3(joints_end),
                            state,
                            src,
                            sub: None,
                        }
                    } else {
                        Op::Linear {
                            start,
                            end: pt3(joints_end),
                            state,
                            src,
                            sub: None,
                        }
                    };
                    self.out.l1_ops.push(op);
                    self.out.rtcp_active = true;
                    self.out.diagnostics.push(Diagnostic {
                        code: ErrorCode::UnsupportedSemantics,
                        stage: Stage::Canonicalize,
                        message: format!(
                            "move lowered to machine-axis joints via MachineKinematics {} ({}, 12.5.4, oracle {}): X/Y/Z op values are machine axes, NOT TCP",
                            self.config.rtcp_mode.name(),
                            self.config.rtcp_mode.inverse_fn(),
                            self.config.rtcp_mode.oracle(),
                        ),
                        src: stmt.src().cloned(),
                        options: vec!["rtcp-trt".to_string()],
                        limit: None,
                    });
                    // Трекинг: программная позиция, machine-оси, углы.
                    self.update_pos_prog(&rp);
                    self.pos_joints = Some(joints_end);
                    self.rot = rot;
                    self.modals
                        .set_motion(if *kind == MoveKind::Rapid { 0 } else { 1 });
                    self.trigger_modal_call(program)?;
                    return Ok(Flow::Normal);
                }

                let trt = TrtProfile::of(&self.config.rtcp_mode);
                if rp.a.is_some() || rp.b.is_some() || rp.c.is_some() {
                    if let Some(profile) = trt {
                        // Ротационный кадр под trt-профилем: joints-оп или
                        // честный носитель (неполная поза / чужое слово).
                        self.exec_trt_move(
                            program,
                            profile,
                            *kind == MoveKind::Rapid,
                            gcode,
                            &rp,
                            feed_v,
                            stmt,
                        )?;
                        return Ok(Flow::Normal);
                    }
                    // Off (MachineKinematics::Triv) → носитель T4: кадр с
                    // подставленными значениями, слова Null опущены (2.1) —
                    // поведение 12.2/12.5.2 БЕЗ trt-профиля («Raw как прежде»).
                    let mut words = frame_words(&rp);
                    if let Some(f) = feed_v {
                        words.push(('F', f));
                    }
                    let text = format_frame(gcode, &words);
                    self.out.push_raw(
                        RawMarker::RotaryAxis,
                        ErrorCode::UnsupportedSemantics,
                        Some(text),
                        stmt,
                        "rotary axis A/B/C not representable in L1 (§3 partial lowering, T4 payload)",
                    );
                    self.update_pos_prog(&rp);
                    return Ok(Flow::Normal);
                }
                // 3) Кадр без ротационных слов: под trt-профилем программа
                //    говорит TCP-координатами (семантика RTCP-контура) — ход
                //    опускается joints-опом на ТЕКУЩИХ углах (трекинг `rot`);
                //    неполная поза → прежний IncompletePose-носитель.
                if let Some(profile) = trt {
                    if self.exec_trt_move(
                        program,
                        profile,
                        *kind == MoveKind::Rapid,
                        gcode,
                        &rp,
                        feed_v,
                        stmt,
                    )? {
                        return Ok(Flow::Normal);
                    }
                }
                // 4) Наследование: слово отсутствует/Null → координата без
                //    изменения (модальные оси); неизвестная позиция +
                //    отсутствующее слово → носитель (E_RUNTIME_STATE —
                //    зависит от состояния станка).
                let cur = self.pos_prog;
                let end_prog = inherit_axes([rp.x, rp.y, rp.z], cur);
                if let Some(ep) = end_prog {
                    // 5) D0-полный: константный И параметрический ход lowered
                    //    (§3 строка 1); координаты — базовая СК (G68/G51).
                    let end_base = self.xform.apply_point(ep);
                    let feed_num = feed_v.map(mk_num4);
                    let state = l1_default_state(feed_num, feed_v.is_some());
                    let src = stmt
                        .src()
                        .cloned()
                        .unwrap_or_else(crate::ast::placeholder_span);
                    let op = match kind {
                        MoveKind::Rapid => Op::Rapid {
                            start: self.pos_start(),
                            end: pt3(end_base),
                            state,
                            src,
                            sub: None,
                        },
                        _ => Op::Linear {
                            start: self.pos_start(),
                            end: pt3(end_base),
                            state,
                            src,
                            sub: None,
                        },
                    };
                    self.out.l1_ops.push(op);
                    self.pos_prog = Some(ep);
                    self.modals.set_motion(match kind {
                        MoveKind::Rapid => 0,
                        _ => 1,
                    });
                    // G66-каркас: триггер модального вызова ПОСЛЕ motion.
                    self.trigger_modal_call(program)?;
                } else {
                    let mut words = frame_words(&rp);
                    if let Some(f) = feed_v {
                        words.push(('F', f));
                    }
                    let text = format_frame(gcode, &words);
                    self.out.push_raw(
                        RawMarker::IncompletePose,
                        ErrorCode::RuntimeState,
                        Some(text),
                        stmt,
                        "target position unknown: missing/Null axis word with unknown current position (address-word semantics, table 2.1)",
                    );
                    // Позиция не обновляется: ход не lowered.
                }
                Ok(Flow::Normal)
            }
            MoveKind::Arc { ijk, r, dir } => {
                // 12.5.3 (trt-профиль): TCP-дуга НЕ аффинно отображается в
                // machine-оси (5-осевая интерполяция — не аффинная геометрия)
                // → честный носитель; NURBS/RTCP-комбинаторика — вне скоупа
                // (док §12.5-амендмент v0.1.5). Off — прежний путь (TCP-дуги).
                if let Some(profile) = TrtProfile::of(&self.config.rtcp_mode) {
                    let words = arc_frame_words(&rp, feed_v);
                    self.out.push_raw(
                        RawMarker::ArcUnlowered,
                        ErrorCode::UnsupportedSemantics,
                        Some(format_frame("G2", &words)),
                        stmt,
                        "arc under MachineKinematics trt profile: TCP arc is not affinely representable in machine-axis moves (5-axis interpolation) — honest carrier; NURBS/RTCP combinatorics out of scope",
                    );
                    self.out.rtcp_active = true;
                    // Линейная компонента дуги меняет TCP даже в носителе
                    // (конвенция 12.2 «линейная компонента трекается в
                    // RAW-ходе») — наследование последующих кадров сходится
                    // с Off-семантикой; machine-позиция после неопущенной
                    // дуги неизвестна.
                    self.update_pos_prog(&rp);
                    // Углы ротационных слов дуги машина выполняет — трекаем.
                    self.rot = profile.update_rot(self.rot, &rp);
                    self.pos_joints = None;
                    return Ok(Flow::Normal);
                }
                self.exec_arc(program, stmt, ijk, r, *dir, &rp, feed_v)
            }
        }
    }

    /// 12.5.3: TCP→joints trt-пересчёт Rapid/Linear-хода под trt-профилем
    /// ([`TrtProfile`], оракул trtfuncs — [`crate::kinematics`]).
    ///
    /// Итог (в порядке проверки):
    /// * чужое ротационное слово (B под XYZAC / A под XYZBC) → честный
    ///   носитель: топология профиля не моделирует ось (head-head и др. —
    ///   при добавлении формул в ядро, док §12.5 v0.1.5);
    /// * ротационный кадр с неполной TCP-позой → носитель
    ///   [`RawMarker::RotaryAxis`] (inverse требует полной позы);
    /// * неполная TCP-поза без ротационных слов → `Ok(false)` — падение на
    ///   прежний IncompletePose-носитель вызывающего (семантика не менялась);
    /// * иначе — joints-оп machine-осей: X/Y/Z = inverse(TCP-поза, углы),
    ///   ротационные слова = joints pass-through + трекинг
    ///   ([`Self::rot`]); start = [`Self::pos_joints`]. per-op диагностика
    ///   («не молча») называет профиль и спец-семантику machine-осей.
    ///
    /// (allow: 8 параметров — сигнатура повторяет структуру кадра хода,
    /// как в [`Self::exec_arc`].)
    #[allow(clippy::too_many_arguments)]
    fn exec_trt_move(
        &mut self,
        program: &L2Program,
        profile: TrtProfile,
        rapid: bool,
        gcode: &str,
        rp: &ResolvedPose,
        feed_v: Option<f64>,
        stmt: &L2Stmt,
    ) -> Result<bool, L2ExecError> {
        let has_rotary = rp.a.is_some() || rp.b.is_some() || rp.c.is_some();
        // Чужая ротационная ось: не в топологии профиля.
        let foreign = match profile.family {
            TrtFamily::Xyzac => rp.b,
            TrtFamily::Xyzbc => rp.a,
        };
        // TCP-конец в программной СК (наследование модальных осей).
        let tcp_end_prog = inherit_axes([rp.x, rp.y, rp.z], self.pos_prog);
        if foreign.is_some() || (has_rotary && tcp_end_prog.is_none()) {
            let mut words = frame_words(rp);
            if let Some(f) = feed_v {
                words.push(('F', f));
            }
            let text = format_frame(gcode, &words);
            let detail = if foreign.is_some() {
                format!(
                    "rotary word {} is outside the MachineKinematics {} topology (12.5.3): joint NOT recomputed, frame kept as T4 carrier (kinematics core covers trt XYZAC/XYZBC only; other topologies need formulas — doc §12.5 v0.1.5)",
                    profile.foreign_letter(),
                    profile.name()
                )
            } else {
                format!(
                    "rotary frame (table axis {}) under MachineKinematics {} (12.5.3): TCP pose incomplete (missing/Null XYZ with unknown current position) — {} requires the full pose, frame kept as T4 carrier",
                    profile.table_letter(),
                    profile.name(),
                    profile.inverse_fn()
                )
            };
            self.out.push_raw(
                RawMarker::RotaryAxis,
                ErrorCode::UnsupportedSemantics,
                Some(text),
                stmt,
                &detail,
            );
            self.out.rtcp_active = true;
            // Линейная компонента трекается даже в носителе (как в Off);
            // углы — только у СВОИХ ротационных слов (чужой кадр семантики
            // профиля не задаёт). Machine-позиция неизвестна (носитель
            // движения).
            self.update_pos_prog(rp);
            if foreign.is_none() {
                self.rot = profile.update_rot(self.rot, rp);
            }
            self.pos_joints = None;
            return Ok(true);
        }
        let Some(ep) = tcp_end_prog else {
            // Без ротационных слов и без позы: прежний IncompletePose-носитель
            // вызывающего (падение сквозь — семантика кадра сохранена).
            return Ok(false);
        };
        // Углы: слова кадра обновляют трекинг (модальность ротационных осей).
        let rot = profile.update_rot(self.rot, rp);
        // TCP-поза в базовой СК (G68/G51-трансформации применяются к
        // линейной части; ротационные углы проходят verbatim — 2D-поворот
        // детали не моделирует ротационные оси).
        let pose_end_base = self.xform.apply_point(ep);
        let joints_end = Self::trt_inverse(profile, pose_end_base, rot);
        let feed_num = feed_v.map(mk_num4);
        let state = l1_default_state(feed_num, feed_v.is_some());
        let src = stmt
            .src()
            .cloned()
            .unwrap_or_else(crate::ast::placeholder_span);
        let start = match self.pos_joints {
            Some(j) => StartPoint::Known(pt3(j)),
            None => StartPoint::Unknown,
        };
        let op = if rapid {
            Op::Rapid {
                start,
                end: pt3(joints_end),
                state,
                src,
                sub: None,
            }
        } else {
            Op::Linear {
                start,
                end: pt3(joints_end),
                state,
                src,
                sub: None,
            }
        };
        self.out.l1_ops.push(op);
        self.out.rtcp_active = true;
        // per-op диагностика «не молча»: L1-оп несёт machine-координаты
        // осей (НЕ TCP) при прежнем заголовке эмиттера — спец-семантика
        // названа (паттерн T6-notice: код UnsupportedSemantics + options).
        self.out.diagnostics.push(Diagnostic {
            code: ErrorCode::UnsupportedSemantics,
            stage: Stage::Canonicalize,
            message: format!(
                "move lowered to machine-axis joints via MachineKinematics {} ({}, 12.5.3, oracle trtfuncs): X/Y/Z op values are machine axes, NOT TCP; rotary words are joint pass-through; forward∘inverse circle invariant holds (kinematics core)",
                profile.name(),
                profile.inverse_fn()
            ),
            src: stmt.src().cloned(),
            options: vec!["rtcp-trt".to_string()],
            limit: None,
        });
        // Трекинг: TCP (программная СК), machine-оси, углы, модальность.
        self.pos_prog = Some(ep);
        self.pos_joints = Some(joints_end);
        self.rot = rot;
        self.modals.set_motion(if rapid { 0 } else { 1 });
        self.trigger_modal_call(program)?;
        Ok(true)
    }

    /// Inverse-кинематика (12.5.3): TCP-поза (базовая СК) + углы
    /// (стол, ротор) → joints machine-осей X/Y/Z. Формулы —
    /// [`crate::kinematics`] (оракул trtfuncs.c; круговой инвариант
    /// forward∘inverse ≡ identity — `tests/kinematics_tests.rs`).
    fn trt_inverse(profile: TrtProfile, xyz_base: [f64; 3], rot: [f64; 3]) -> [f64; 3] {
        match profile.family {
            TrtFamily::Xyzac => {
                let pose = Pose6::new(xyz_base[0], xyz_base[1], xyz_base[2], rot[0], 0.0, rot[2]);
                let j = xyzac_inverse(&profile.params, &pose);
                [j.x, j.y, j.z]
            }
            TrtFamily::Xyzbc => {
                let pose = Pose6::new(xyz_base[0], xyz_base[1], xyz_base[2], 0.0, rot[1], rot[2]);
                let j = xyzbc_inverse(&profile.params, &pose);
                [j.x, j.y, j.z]
            }
        }
    }

    /// Дуга → [`Op::Arc`] (канон eid-core) или честный носитель.
    /// (allow: 8 параметров — сигнатура повторяет структуру кадра дуги
    /// §2.4; группировка в структуру добавила бы аллокацию на ход.)
    #[allow(clippy::too_many_arguments)]
    fn exec_arc(
        &mut self,
        program: &L2Program,
        stmt: &L2Stmt,
        ijk: &[Option<Expr>; 3],
        r: &Option<Expr>,
        dir: Dir,
        rp: &ResolvedPose,
        feed_v: Option<f64>,
    ) -> Result<Flow, L2ExecError> {
        // 12.5.4: дуга под НЕлинейной kinematics-топологией (XyzabTdr/
        // Trsrn/Maxkins/FiveAxis) → честный носитель с маркировкой профиля
        // (TCP-дуга не аффинно отображается в machine-оси; 5-осевая
        // интерполяция — не аффинная геометрия — граница 12.5.3). До
        // start-проверки: маркировка не зависит от разрешимости геометрии.
        if matches!(
            self.config.rtcp_mode,
            RtcpMode::XyzabTdr { .. }
                | RtcpMode::Trsrn { .. }
                | RtcpMode::Maxkins { .. }
                | RtcpMode::FiveAxis { .. }
        ) {
            let words = arc_frame_words(rp, feed_v);
            let name = self.config.rtcp_mode.name();
            self.out.push_raw(
                RawMarker::ArcUnlowered,
                ErrorCode::UnsupportedSemantics,
                Some(format_frame("G2", &words)),
                stmt,
                &format!(
                    "arc under MachineKinematics {name} profile (12.5.4): TCP arc is not affinely representable in machine-axis moves (5-axis interpolation) — honest carrier"
                ),
            );
            self.out.rtcp_active = true;
            return Ok(Flow::Normal);
        }
        // Старт в базовой СК обязателен: центр IJK = старт + смещения.
        let Some(sp) = self.pos_prog else {
            let words = arc_frame_words(rp, feed_v);
            self.out.push_raw(
                RawMarker::ArcUnlowered,
                ErrorCode::RuntimeState,
                Some(format_frame("G2", &words)),
                stmt,
                "arc start position unknown — geometry not derivable",
            );
            return Ok(Flow::Normal);
        };
        let start_base = self.xform.apply_point(sp);
        // Конец с наследованием.
        let end_prog = inherit_axes([rp.x, rp.y, rp.z], Some(sp));
        let Some(ep) = end_prog else {
            let words = arc_frame_words(rp, feed_v);
            self.out.push_raw(
                RawMarker::ArcUnlowered,
                ErrorCode::RuntimeState,
                Some(format_frame("G2", &words)),
                stmt,
                "arc end position unknown (missing/Null word with unknown current position)",
            );
            return Ok(Flow::Normal);
        };
        let end_base = self.xform.apply_point(ep);
        // Слова I/J/K (отсутствующее/Null слово = 0.0, канон RS274).
        let al = plane_linear(self.modals.plane());
        let mut offs = [0.0f64; 3];
        let mut has_in_plane = false;
        for (idx, e) in ijk.iter().enumerate() {
            if let Some(e) = e {
                if let Some(v) = self.eval_resolved(e)?.resolve_word() {
                    offs[idx] = v;
                    if idx != al {
                        has_in_plane = true;
                    }
                }
            }
        }
        let r_v = match r {
            Some(e) => self.eval_resolved(e)?.resolve_word(),
            None => None,
        };
        // Зеркало/отрицательный масштаб (det < 0) → Cw↔Ccw (T6/§3 строка 4).
        let det = self.xform.det();
        let dir_eff = if det < 0.0 {
            match dir {
                Dir::Cw => Dir::Ccw,
                Dir::Ccw => Dir::Cw,
            }
        } else {
            dir
        };
        let plane = self.modals.plane();
        let geom = if let Some(rv) = r_v {
            // Радиус масштабируется линейной частью (|det|^(1/3) для
            // uniform-scale; поворот — 1).
            let s = det.abs().cbrt();
            arc_from_r(start_base, end_base, rv * s, plane, dir_eff)
        } else if has_in_plane {
            // Смещения — ВЕКТОРЫ: поворот/масштаб/зеркало без переноса.
            arc_from_ijk(
                start_base,
                end_base,
                self.xform.apply_vec(offs),
                plane,
                dir_eff,
            )
        } else {
            Err(ArcGeomError::NoGeometry)
        };
        match geom {
            Ok(g) => {
                let feed_num = feed_v.map(mk_num4);
                let state = l1_default_state(feed_num, feed_v.is_some());
                let src = stmt
                    .src()
                    .cloned()
                    .unwrap_or_else(crate::ast::placeholder_span);
                let op = Op::Arc {
                    start: StartPoint::Known(pt3(start_base)),
                    end: pt3(end_base),
                    center: pt3(g.center),
                    normal: pt3(g.normal),
                    dir: dir_eff,
                    sweep_deg: mk_num4(g.sweep_deg),
                    pitch: mk_num4(g.pitch),
                    state,
                    src,
                    sub: None,
                };
                self.out.l1_ops.push(op);
                self.pos_prog = Some(ep);
                self.modals.set_motion(match dir_eff {
                    Dir::Cw => 2,
                    Dir::Ccw => 3,
                });
                self.trigger_modal_call(program)?;
            }
            Err(reason) => {
                let words = arc_frame_words(rp, feed_v);
                let detail = match &reason {
                    ArcGeomError::NoGeometry => {
                        "arc without in-plane IJK/R words — geometry not derivable".to_string()
                    }
                    ArcGeomError::OffPlaneComponent { letter, value } => format!(
                        "perpendicular arc-center component {letter}{value} — center must lie in start plane (SPEC §6)"
                    ),
                    ArcGeomError::ChordTooLarge { chord, r } => format!(
                        "arc chord {chord:.4} > 2R {r}: impossible arc (SPEC §6)"
                    ),
                    ArcGeomError::RAmbiguousFullCircle => {
                        "R-arc with coincident in-plane start/end: ambiguous center (SPEC §6)"
                            .to_string()
                    }
                };
                self.out.push_raw(
                    RawMarker::ArcUnlowered,
                    ErrorCode::InvalidInput,
                    Some(format_frame("G2", &words)),
                    stmt,
                    &detail,
                );
                // Позиция не обновляется (ход не lowered — носитель).
            }
        }
        Ok(Flow::Normal)
    }

    /// Обновить программную позицию словами носителя ротационного хода
    /// (линейная компонента меняет координаты даже в RAW-ходе).
    fn update_pos_prog(&mut self, rp: &ResolvedPose) {
        let cur = self.pos_prog;
        if let Some(p) = inherit_axes([rp.x, rp.y, rp.z], cur) {
            self.pos_prog = Some(p);
        }
    }

    // ----- CallSub (D1) -----------------------------------------------------

    /// Индекс локали для буквы args (BUG-D1 фикс, амендмент v0.1.2).
    ///
    /// Узел `CallSub` не несёт диалекта, а семантика букв различна:
    ///  * Macro B G65/G66/M98 (fanuc/haas) — ИМЕНОВАННАЯ НЕлинейная
    ///    Fanuc-таблица (`ast::call_arg_local_index`: D→#7, F→#9, I→#4…);
    ///  * LinuxCNC `o<…> call [a][b]…` — ПОЗИЦИОННАЯ (парсер LinuxCNC
    ///    синтезирует буквы 'A'+i в порядке следования; k-я → #(k+1),
    ///    `ast::positional_arg_local_index`).
    ///
    /// Различитель: список букв args = ТОЧНЫЙ префикс 'A','B','C',… —
    /// формат синтеза LinuxCNC → позиционная таблица; любой иной набор
    /// букв → Fanuc-таблица. Буквы A/B/C совпадают в обеих таблицах.
    /// ОГОВОРКА (документировано, док §2.4 амендмент v0.1.2): Fanuc-вызов
    /// с буквами РОВНО A..N-префикса из ≥4 аргументов (напр. `G65 A1. B1.
    /// C1. D1.`) свяжется позиционно (D→#4 вместо Fanuc-#7) — каноничный
    /// фикс (поле диалекта в CallSub) требует амендмента AST вне зоны B2;
    /// в корпусе таких вызовов нет (ценз G65-args: deep-drill Z/D/S/F/E,
    /// probes X/Z/S, material-verify Z/M — префиксы не образуют).
    fn call_arg_index(args: &[(u8, crate::ast::Expr)], letter: u8) -> Option<u32> {
        let positional = args
            .iter()
            .enumerate()
            .all(|(i, (l, _))| l.to_ascii_uppercase() == b'A' + i as u8);
        if positional {
            crate::ast::positional_arg_local_index(letter)
        } else {
            crate::ast::call_arg_local_index(letter)
        }
    }

    /// CallSub: стек кадров глубина 9, args → локали по таблице диалекта
    /// (BUG-D1 фикс: Macro B G65 — Fanuc-таблица, LinuxCNC o-call —
    /// позиционная; см. [`Self::call_arg_index`]), Null пробрасывается,
    /// repeat L/K, modal G66 — каркас.
    fn exec_call_sub(&mut self, program: &L2Program, stmt: &L2Stmt) -> Result<Flow, L2ExecError> {
        let (target, repeat, args, modal) = match stmt {
            L2Stmt::CallSub {
                target,
                repeat,
                args,
                modal,
                ..
            } => (target, repeat, args, modal),
            _ => unreachable!("exec_call_sub called on non-CallSub"),
        };
        // G66 (модальный вызов) — каркас: регистрация цели и ЗНАЧЕНИЙ
        // аргументов (вычислены здесь, контекст вызывающего); триггер —
        // после каждого lowered motion-опа. G67 (отмена) не имеет узла
        // AST §2.4 — контракт-пробел (шапка).
        if *modal {
            let mut bound = Vec::with_capacity(args.len());
            for (letter, value) in args {
                let idx = Self::call_arg_index(args, *letter).ok_or_else(|| {
                    L2ExecError::Eval(L2Error::RuntimeError(format!(
                        "CALL argument letter binds to no local slot (G65 Fanuc arg table / positional), got {letter:#x}"
                    )))
                })?;
                let v = self.eval_resolved(value)?;
                bound.push((idx, v));
            }
            self.modal_call = Some((target.clone(), bound));
            let detail = format!(
                "modal call (G66) registered: {} ({} args); trigger after motion; G67 cancel has no AST node yet (skeleton)",
                sub_target_name(target),
                args.len()
            );
            self.out.push_raw(
                RawMarker::SubCall,
                ErrorCode::UnsupportedSemantics,
                Some("G66\n".to_string()),
                stmt,
                &detail,
            );
            return Ok(Flow::Normal);
        }
        // Число повторений (L/K-слово; Null/отсутствует → 1).
        let times = match repeat {
            Some(e) => {
                let v = self.eval_resolved(e)?;
                let n = v.as_num();
                if !n.is_finite() || n < 0.0 {
                    return Err(L2ExecError::Eval(L2Error::RuntimeError(format!(
                        "negative/non-finite CALL repeat count: {n}"
                    ))));
                }
                n.trunc() as u64
            }
            None => 1,
        };
        // Аргументы вычисляются в КОНТЕКСТЕ ВЫЗЫВАЮЩЕГО (в т.ч. Null —
        // таблица 2.1: опциональный аргумент пробрасывается как Null).
        let mut bound = Vec::with_capacity(args.len());
        for (letter, value) in args {
            let idx = Self::call_arg_index(args, *letter).ok_or_else(|| {
                L2ExecError::Eval(L2Error::RuntimeError(format!(
                    "CALL argument letter binds to no local slot (G65 Fanuc arg table / positional), got {letter:#x}"
                )))
            })?;
            let v = self.eval_resolved(value)?;
            bound.push((idx, v));
        }
        for _ in 0..times {
            self.call_frame(program, target, &bound)?;
        }
        Ok(Flow::Normal)
    }

    /// Один вызов: guard глубины 9 → push кадра → привязка args → тело
    /// (неявный return на конце тела ИЛИ явный M99/Return) → pop.
    fn call_frame(
        &mut self,
        program: &L2Program,
        target: &SubTarget,
        bound: &[(u32, Val)],
    ) -> Result<(), L2ExecError> {
        let sub = program.find_sub(target).ok_or_else(|| {
            L2ExecError::Eval(L2Error::RuntimeError(format!(
                "subprogram target not found: {}",
                sub_target_name(target)
            )))
        })?;
        // Дизайн §5 12.2: глубина стека вызовов до 9 (корневой кадр не
        // считается вызовом: 9 АКТИВНЫХ вызовов допустимы, 10-й — ошибка).
        if self.env.frame_depth() > self.config.max_call_depth {
            return Err(L2ExecError::CallDepthExceeded {
                limit: self.config.max_call_depth,
            });
        }
        let body_labels = self.labels_of(&sub.body);
        self.env.push_frame();
        for (idx, v) in bound {
            self.env.write_direct(*idx, *v)?;
        }
        let res = self.exec_block(program, &sub.body, &body_labels);
        self.env.pop_frame()?;
        // Return (M99) поглощается на границе вызова; конец тела без M99 —
        // неявный return (та же семантика pop).
        match res? {
            Flow::Normal | Flow::Return => Ok(()),
            Flow::Jump(_) => unreachable!("Jump must be absorbed by exec_block"),
        }
    }

    /// G66-триггер после lowered motion-опа (каркас §5 12.2).
    fn trigger_modal_call(&mut self, program: &L2Program) -> Result<(), L2ExecError> {
        let Some((target, bound)) = self.modal_call.clone() else {
            return Ok(());
        };
        self.call_frame(program, &target, &bound)
    }
}

// ===========================================================================
// Вспомогательные функции
// ===========================================================================

/// Слова кадра для payload-носителя (порядок X Y Z A B C; только
/// присутствующие и не-Null — адресная семантика 2.1; T4: «X.. A..»).
fn frame_words(rp: &ResolvedPose) -> Vec<(char, f64)> {
    let mut w = Vec::new();
    for (letter, v) in [
        ('X', rp.x),
        ('Y', rp.y),
        ('Z', rp.z),
        ('A', rp.a),
        ('B', rp.b),
        ('C', rp.c),
    ] {
        if let Some(v) = v {
            w.push((letter, v));
        }
    }
    w
}

/// Слова кадра дуги для payload-носителя: X..C + F. NOTE: значения IJK/R
/// намеренно не повторяются в payload (геометрия уже проанализирована,
/// честная причина — в reason носителя; lossless-дамп оператора доступен
/// через легаси-путь push_raw).
fn arc_frame_words(rp: &ResolvedPose, feed: Option<f64>) -> Vec<(char, f64)> {
    let mut w = frame_words(rp);
    if let Some(f) = feed {
        w.push(('F', f));
    }
    w
}

/// Отображение Axis → индекс компоненты 0..3 для зеркала (X/Y/Z);
/// ротационные/доп. оси — ошибка (не аффинны в 3D-модели Pass-1).
fn mirror_axis_index(axis: &Axis) -> Result<usize, L2ExecError> {
    match axis {
        Axis::X => Ok(0),
        Axis::Y => Ok(1),
        Axis::Z => Ok(2),
        other => Err(L2ExecError::UnsupportedSemantics(format!(
            "mirror on non-linear axis {} is not an affine 3D transform (Pass-1)",
            other.letter()
        ))),
    }
}

/// Имя Sys-переменной для сообщений (sync с eval::sys_var_name).
fn sys_var_display(sv: &SysVar) -> String {
    match sv {
        SysVar::AbsMachinePos(a) => format!("#ABS[{}]", a.letter()),
        SysVar::WorkPos(a) => format!("#WORK[{}]", a.letter()),
        SysVar::SkipPos(a) => format!("#SKIP[{}]", a.letter()),
        SysVar::ModalGroup(g) => format!("#MODAL[{g}]"),
        SysVar::WcsOffset { p, axis } => format!("#WCS[{p}.{}]", axis.letter()),
        SysVar::VendorSpecific(n) => format!("#VENDOR[{n}]"),
    }
}

/// D2: рекурсивное предвычисление per-body таблиц меток (узлы Label).
fn precompute_labels(stmts: &[L2Stmt], map: &mut HashMap<usize, BTreeMap<u32, usize>>) {
    map.insert(
        stmts.as_ptr() as usize,
        crate::lowering::collect_labels(stmts),
    );
    for stmt in stmts {
        match stmt {
            L2Stmt::While { body, .. } | L2Stmt::WithTransform { body, .. } => {
                precompute_labels(body, map);
            }
            _ => {}
        }
    }
}

// ===========================================================================
// Тесты: D0–D3-матрица, T4/T5/T6-пины, §3-матрица, трансформации, SysVar
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ast::{
        placeholder_span, Axis, BinaryOp, Plane, SubTarget, SysVar, TransformKind, UnaryOp, VarRef,
    };
    use eid_core::ast::Dir;

    fn cfg() -> L2Config {
        L2Config::default()
    }

    fn ev() -> L2Evaluator {
        L2Evaluator::new(cfg())
    }

    fn target_const(x: f64, y: f64, z: f64) -> ExprPose {
        ExprPose {
            x: Some(Expr::num(x)),
            y: Some(Expr::num(y)),
            z: Some(Expr::num(z)),
            a: None,
            b: None,
            c: None,
        }
    }

    fn assign(idx: u32, v: Expr) -> L2Stmt {
        L2Stmt::Assign {
            target: VarRef::Direct(idx),
            value: v,
            src: placeholder_span(),
        }
    }

    fn move_stmt(kind: MoveKind, target: ExprPose, feed: Option<Expr>) -> L2Stmt {
        L2Stmt::Move {
            kind,
            target,
            feed,
            spindle: None,
            src: placeholder_span(),
        }
    }

    fn call(target: SubTarget, args: Vec<(u8, Expr)>) -> L2Stmt {
        L2Stmt::CallSub {
            target,
            repeat: None,
            args,
            modal: false,
            src: placeholder_span(),
        }
    }

    fn label(n: u32) -> L2Stmt {
        L2Stmt::Label {
            n,
            src: placeholder_span(),
        }
    }

    fn goto(n: u32, cond: Expr) -> L2Stmt {
        L2Stmt::IfGoto {
            cond,
            target_label: n,
            src: placeholder_span(),
        }
    }

    fn linear(x: Expr, y: Expr, z: Expr, feed: Option<Expr>) -> L2Stmt {
        move_stmt(
            MoveKind::Linear,
            ExprPose {
                x: Some(x),
                y: Some(y),
                z: Some(z),
                a: None,
                b: None,
                c: None,
            },
            feed,
        )
    }

    /// Разкодировать hex-payload носителя в текст (обратимость §3/T4).
    fn decode_payload(op: &Op) -> String {
        match op {
            Op::Raw { payload_hex, .. } => {
                let bytes: Vec<u8> = (0..payload_hex.len() / 2)
                    .map(|i| u8::from_str_radix(&payload_hex[i * 2..i * 2 + 2], 16).unwrap())
                    .collect();
                String::from_utf8(bytes).unwrap()
            }
            _ => panic!("expected Raw"),
        }
    }

    // ---------- T1: L1-программа проходит конвейер 1:1 ----------

    #[test]
    fn t1_l1_only_program_passes_through() {
        let l1 = vec![
            Op::Rapid {
                start: StartPoint::Unknown,
                end: Point3 {
                    x: Num::new("0".into(), 0.0),
                    y: Num::new("0".into(), 0.0),
                    z: Num::new("5".into(), 5.0),
                },
                state: l1_default_state(None, false),
                src: placeholder_span(),
                sub: None,
            },
            Op::ProgramEnd {
                src: placeholder_span(),
            },
        ];
        let program = L2Program::new(l1.iter().cloned().map(L2Stmt::L1Op).collect(), Vec::new());
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert_eq!(res.l1_ops, l1); // passthrough 1:1
        assert!(res.raw_tail.is_empty());
        assert!(res.diagnostics.is_empty());
    }

    // ---------- D0: Assign / Move / позиция ----------

    #[test]
    fn d0_assign_and_null_copy() {
        // #100 = 5; #101 = #0 (Null копируется); #102 = -#100.
        let program = L2Program::new(
            vec![
                assign(100, Expr::num(5.0)),
                assign(101, Expr::Null),
                assign(102, Expr::Unary(UnaryOp::Neg, Box::new(Expr::var(100)))),
            ],
            Vec::new(),
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(100).unwrap(), Val::Num(5.0));
        // ПИН (таблица 2.1, строка 1): присваивание копирует именно Null.
        assert_eq!(e.env().read_direct(101).unwrap(), Val::Null);
        assert_eq!(e.env().read_direct(102).unwrap(), Val::Num(-5.0));
    }

    #[test]
    fn d0_const_and_parametric_move_resolved_to_l1() {
        // §3 строка 1: `#100 = 5, G0 X#100` → Op::Rapid X5.0000.
        // Параметрический ход РАЗРЕШАЕТСЯ (12.2), числа 4dp.
        let program = L2Program::new(
            vec![
                assign(100, Expr::num(15.0)),
                move_stmt(
                    MoveKind::Rapid,
                    ExprPose {
                        x: Some(Expr::var(100)),
                        y: Some(Expr::num(2.0)),
                        z: Some(Expr::num(3.0)),
                        a: None,
                        b: None,
                        c: None,
                    },
                    None,
                ),
                linear(
                    Expr::Binary(
                        BinaryOp::Add,
                        Box::new(Expr::var(100)),
                        Box::new(Expr::num(5.0)),
                    ),
                    Expr::num(0.0),
                    Expr::num(5.0),
                    Some(Expr::num(120.0)),
                ),
            ],
            Vec::new(),
        );
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 2);
        assert!(res.raw_tail.is_empty());
        match (&res.l1_ops[0], &res.l1_ops[1]) {
            (
                Op::Rapid { end, start, .. },
                Op::Linear {
                    end: e2,
                    state,
                    start: s2,
                    ..
                },
            ) => {
                assert_eq!(
                    (end.x.value(), end.y.value(), end.z.value()),
                    (15.0, 2.0, 3.0)
                );
                assert_eq!((e2.x.value(), e2.y.value(), e2.z.value()), (20.0, 0.0, 5.0));
                assert_eq!(state.feed.as_ref().map(|f| f.value()), Some(120.0));
                // Трекинг позиции: первый ход Unknown (позиция неизвестна),
                // второй Known = конец первого (связность канонической формы).
                assert!(matches!(start, StartPoint::Unknown));
                assert!(matches!(s2, StartPoint::Known(_)));
            }
            _ => panic!("expected Rapid+Linear"),
        }
        // 4dp-текст (SPEC §10.5 single-final-rounding).
        if let Op::Linear { end, .. } = &res.l1_ops[1] {
            assert_eq!(end.x.text, "20.0000");
        }
    }

    #[test]
    fn d0_uninitialized_var_word_is_null_ignored() {
        // #100 не инициализирована → Null → слово X игнорируется; позиция
        // неизвестна → носитель E_RUNTIME_STATE (адресная семантика 2.1).
        let program = L2Program::new(
            vec![linear(Expr::var(100), Expr::num(0.0), Expr::num(5.0), None)],
            Vec::new(),
        );
        let res = ev().run(&program).unwrap();
        assert!(res.l1_ops.is_empty());
        assert_eq!(res.raw_tail[0].marker, RawMarker::IncompletePose);
        assert_eq!(res.diagnostics[0].code, ErrorCode::RuntimeState);
    }

    #[test]
    fn d0_word_inheritance_from_tracked_position() {
        // G0 X5 Y0 Z0; G0 X10 (Y/Z модально наследуются) → оба хода lowered.
        let program = L2Program::new(
            vec![
                move_stmt(MoveKind::Rapid, target_const(5.0, 0.0, 0.0), None),
                move_stmt(
                    MoveKind::Rapid,
                    ExprPose {
                        x: Some(Expr::num(10.0)),
                        y: None,
                        z: None,
                        a: None,
                        b: None,
                        c: None,
                    },
                    None,
                ),
            ],
            Vec::new(),
        );
        let res = ev().run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 2);
        assert!(res.raw_tail.is_empty());
        if let Op::Rapid { end, start, .. } = &res.l1_ops[1] {
            assert_eq!(
                (end.x.value(), end.y.value(), end.z.value()),
                (10.0, 0.0, 0.0)
            );
            // start Known = конец предыдущего хода (точка разрыва (a) закрыта).
            match start {
                StartPoint::Known(p) => assert_eq!((p.x.value(), p.y.value()), (5.0, 0.0)),
                StartPoint::Unknown => panic!("start must be known after first move"),
            }
        } else {
            panic!("expected Rapid");
        }
    }

    #[test]
    fn d0_initial_pos_config_enables_first_partial_move() {
        // initial_pos (MachineProfile/симулятор-хоум) → наследование с кадра 1.
        let cfg = L2Config {
            initial_pos: Some([0.0, 0.0, 0.0]),
            ..cfg()
        };
        let program = L2Program::new(
            vec![move_stmt(
                MoveKind::Rapid,
                ExprPose {
                    x: None,
                    y: None,
                    z: Some(Expr::num(1.0)),
                    a: None,
                    b: None,
                    c: None,
                },
                None,
            )],
            Vec::new(),
        );
        let res = L2Evaluator::new(cfg).run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 1);
        if let Op::Rapid { end, .. } = &res.l1_ops[0] {
            assert_eq!(
                (end.x.value(), end.y.value(), end.z.value()),
                (0.0, 0.0, 1.0)
            );
        }
    }

    #[test]
    fn d0_feed_null_word_ignored_and_restated_flag() {
        // F#101 (Null) → F-слова нет (2.1); константный F → флаг restated.
        let mk = |feed: Option<Expr>| {
            L2Program::new(
                vec![linear(Expr::num(1.0), Expr::num(0.0), Expr::num(0.0), feed)],
                Vec::new(),
            )
        };
        let res = ev().run(&mk(Some(Expr::var(101)))).unwrap();
        assert_eq!(res.l1_ops.len(), 1);
        if let Op::Linear { state, .. } = &res.l1_ops[0] {
            assert!(state.feed.is_none());
            assert!(!state.feed_restated);
        }
        let res = ev().run(&mk(Some(Expr::num(50.0)))).unwrap();
        if let Op::Linear { state, .. } = &res.l1_ops[0] {
            assert_eq!(state.feed.as_ref().map(|f| f.value()), Some(50.0));
            assert!(state.feed_restated);
        }
    }

    #[test]
    fn d0_spindle_word_does_not_block_lowering() {
        // §2.4 Move.spindle: S-слово (константное и параметрическое) не
        // блокирует lowering; в State не пишется (нет M3/M4-контекста — шапка).
        let mk = |spindle: Option<Expr>| {
            L2Program::new(
                vec![L2Stmt::Move {
                    kind: MoveKind::Rapid,
                    target: target_const(1.0, 2.0, 3.0),
                    feed: None,
                    spindle,
                    src: placeholder_span(),
                }],
                Vec::new(),
            )
        };
        for s in [Some(Expr::num(1200.0)), Some(Expr::var(500))] {
            let res = ev().run(&mk(s)).unwrap();
            assert_eq!(res.l1_ops.len(), 1, "S must not block lowering");
            assert!(res.raw_tail.is_empty());
            if let Op::Rapid { state, .. } = &res.l1_ops[0] {
                assert_eq!(state.spindle, eid_core::ast::Spindle::Off);
            }
        }
    }

    #[test]
    fn d0_null_word_ignored_incomplete_pose() {
        // Y = Null → слово игнорируется (таблица 2.1) → поза неполная при
        // неизвестной позиции → носитель.
        let program = L2Program::new(
            vec![move_stmt(
                MoveKind::Rapid,
                ExprPose {
                    x: Some(Expr::num(1.0)),
                    y: Some(Expr::Null),
                    z: Some(Expr::num(3.0)),
                    a: None,
                    b: None,
                    c: None,
                },
                None,
            )],
            Vec::new(),
        );
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert!(res.l1_ops.is_empty());
        assert_eq!(res.raw_tail[0].marker, RawMarker::IncompletePose);
    }

    // ---------- T4: частичный lowering ротационной оси (ДОСЛОВНО) ----------

    #[test]
    fn t4_partial_lowering_rotary_payload_verbatim() {
        // Чек-лист T4: #100 = 15; G0 X#100 A#100 → Op::Raw с payload
        // «G0 X15.0000 A15.0000\n» (hex, с терминатором), код
        // E_UNSUPPORTED_SEMANTICS (§3 строка 5).
        let program = L2Program::new(
            vec![
                assign(100, Expr::num(15.0)),
                move_stmt(
                    MoveKind::Rapid,
                    ExprPose {
                        x: Some(Expr::var(100)),
                        y: None,
                        z: None,
                        a: Some(Expr::var(100)),
                        b: None,
                        c: None,
                    },
                    None,
                ),
            ],
            Vec::new(),
        );
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert!(res.l1_ops.is_empty());
        assert_eq!(res.raw_tail.len(), 1);
        assert_eq!(res.raw_tail[0].marker, RawMarker::RotaryAxis);
        match &res.raw_tail[0].op {
            Op::Raw { code, .. } => assert_eq!(*code, ErrorCode::UnsupportedSemantics),
            _ => panic!("carrier must be Raw"),
        }
        assert_eq!(
            decode_payload(&res.raw_tail[0].op),
            "G0 X15.0000 A15.0000\n"
        );
        // Диагностика носителя со спаном оператора (§2.4).
        assert_eq!(res.diagnostics.len(), 1);
        assert!(res.diagnostics[0].src.is_some());
    }

    #[test]
    fn t4_linear_rotary_payload_and_pos_update() {
        // G1-носитель + линейная компонента обновляет позицию: следующий
        // ход наследует X из RAW-хода.
        let program = L2Program::new(
            vec![
                move_stmt(MoveKind::Rapid, target_const(10.0, 0.0, 0.0), None),
                move_stmt(
                    MoveKind::Linear,
                    ExprPose {
                        x: Some(Expr::num(20.0)),
                        y: Some(Expr::num(0.0)),
                        z: Some(Expr::num(0.0)),
                        a: Some(Expr::num(90.0)),
                        b: None,
                        c: None,
                    },
                    Some(Expr::num(300.0)),
                ),
                move_stmt(
                    MoveKind::Rapid,
                    ExprPose {
                        x: None,
                        y: Some(Expr::num(5.0)),
                        z: None,
                        a: None,
                        b: None,
                        c: None,
                    },
                    None,
                ),
            ],
            Vec::new(),
        );
        let res = ev().run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 2); // первый и третий ходы
        assert_eq!(res.raw_tail.len(), 1);
        assert_eq!(
            decode_payload(&res.raw_tail[0].op),
            "G1 X20.0000 Y0.0000 Z0.0000 A90.0000 F300.0000\n"
        );
        // Третий ход: X наследуется 20 (обновлено RAW-ходом), Y=5, Z → 0.
        if let Op::Rapid { end, .. } = &res.l1_ops[1] {
            assert_eq!(
                (end.x.value(), end.y.value(), end.z.value()),
                (20.0, 5.0, 0.0)
            );
        }
    }

    // ---------- T8: RTCP-профиль lowering (12.5.2) ---------------------------

    /// XyzacTrt-конфиг: геометрия станка по умолчанию ([`TrtParams`] — ядро
    /// 12.5.1; САМИ параметры на lowering 12.5.2 не влияют — joints не
    /// пересчитываются, носитель не опускается; полный TCP→joints — 12.5.3).
    fn cfg_trt() -> L2Config {
        L2Config {
            rtcp_mode: RtcpMode::XyzacTrt {
                params: TrtParams::new([0.0, 0.0, 0.0], 0.0, 0.0, 0.0, false),
            },
            ..cfg()
        }
    }

    /// `#100 = 15; G0 A#100` — ПЕРЕМЕННАЯ в ротационном слове.
    fn rotary_var_program() -> L2Program {
        L2Program::new(
            vec![
                assign(100, Expr::num(15.0)),
                move_stmt(
                    MoveKind::Rapid,
                    ExprPose {
                        x: None,
                        y: None,
                        z: None,
                        a: Some(Expr::var(100)),
                        b: None,
                        c: None,
                    },
                    None,
                ),
            ],
            Vec::new(),
        )
    }

    #[test]
    fn t8_rtcp_off_rotary_var_resolved_into_raw_pin() {
        // ПИН 12.2: rtcp=Off (дефолт) — переменная ротационного слова
        // разрешается в T4-носитель с подставленным значением
        // («G0 A15.0000\n»); rtcp_active = false (маркировки нет).
        let res = ev().run(&rotary_var_program()).unwrap();
        assert!(res.l1_ops.is_empty());
        assert_eq!(res.raw_tail.len(), 1);
        assert_eq!(res.raw_tail[0].marker, RawMarker::RotaryAxis);
        assert_eq!(decode_payload(&res.raw_tail[0].op), "G0 A15.0000\n");
        assert!(!res.rtcp_active);
    }

    #[test]
    fn t8_rtcp_xyzac_trt_same_carrier_marked_active() {
        // 12.5.2: XyzacTrt — ТОТ ЖЕ Raw-носитель (T4-паттерн), НО результат
        // помечен rtcp_active = true + диагностика называет профиль и
        // границу 12.5.3 («не молча»). Код носителя — честный
        // E_UNSUPPORTED_SEMANTICS: носитель НЕ опущен, joints НЕ
        // пересчитаны.
        let res = L2Evaluator::new(cfg_trt())
            .run(&rotary_var_program())
            .unwrap();
        assert!(res.l1_ops.is_empty());
        assert_eq!(res.raw_tail.len(), 1);
        assert_eq!(res.raw_tail[0].marker, RawMarker::RotaryAxis);
        assert_eq!(decode_payload(&res.raw_tail[0].op), "G0 A15.0000\n");
        assert!(res.rtcp_active);
        match &res.raw_tail[0].op {
            Op::Raw { code, .. } => assert_eq!(*code, ErrorCode::UnsupportedSemantics),
            _ => panic!("carrier must be Raw"),
        }
        // «Не молча»: диагностика носителя содержит имя профиля и фазу 12.5.3.
        assert!(res
            .diagnostics
            .iter()
            .any(|d| d.message.contains("XyzacTrt") && d.message.contains("12.5.3")));
    }

    #[test]
    fn t8_rtcp_off_carries_no_rtcp_marking() {
        // Анти-пин «не молча»: при Off НЕТ ни флага, ни RTCP-сообщений —
        // диагностика носителя прежняя (без имени профиля), ложной
        // маркировки не возникает.
        let res = ev().run(&rotary_var_program()).unwrap();
        assert!(!res.rtcp_active);
        assert!(res
            .diagnostics
            .iter()
            .all(|d| !d.message.contains("XyzacTrt")));
    }

    #[test]
    fn t8_rtcp_trt_t4_mixed_frame_verbatim_both_modes() {
        // T4 ДОСЛОВНО в обоих режимах: #100 = 15; G0 X#100 A#100 →
        // payload «G0 X15.0000 A15.0000\n». Линейная часть НЕ опускается в
        // L1-оп даже под XyzacTrt (ход X/Y/Z+A/C без пересчёта joints
        // изменил бы TCP-геометрию — граница 12.5.3); различие режимов —
        // ТОЛЬКО флаг rtcp_active.
        let program = L2Program::new(
            vec![
                assign(100, Expr::num(15.0)),
                move_stmt(
                    MoveKind::Rapid,
                    ExprPose {
                        x: Some(Expr::var(100)),
                        y: None,
                        z: None,
                        a: Some(Expr::var(100)),
                        b: None,
                        c: None,
                    },
                    None,
                ),
            ],
            Vec::new(),
        );
        let off = ev().run(&program).unwrap();
        let trt = L2Evaluator::new(cfg_trt()).run(&program).unwrap();
        for res in [&off, &trt] {
            assert_eq!(res.l1_ops.len(), 0);
            assert_eq!(res.raw_tail.len(), 1);
            assert_eq!(
                decode_payload(&res.raw_tail[0].op),
                "G0 X15.0000 A15.0000\n"
            );
        }
        assert!(!off.rtcp_active);
        assert!(trt.rtcp_active);
    }

    #[test]
    fn t8_rtcp_trt_rotary_word_expression_resolved_into_joints() {
        // 12.5.3: переменные в ротационных словах разрешаются ТАК ЖЕ, как
        // линейные (12.5.2-пин), НО ход с полной TCP-позой теперь опускается
        // joints-опом machine-осей: initial_pos home (0,0,0), углы home
        // (0,0), #101 = #100 * 2 → C30; TCP-поза (10, 0, 0, A=0, C=30) →
        // xyzac_inverse (нулевая геометрия, con=−1):
        //   jx = cos30·10 = 8.6603; jy = sin30·10 = 5.0; jz = 0.
        // Следующий кадр G0 Y7 — БЕЗ ротационных слов: полный-TCP режим —
        // тоже joints-оп на ТЕКУЩИХ углах (A=0, C=30):
        //   pose (10, 7, 0): jx = cos30·10 − sin30·7 = 5.1603;
        //   jy = sin30·10 + cos30·7 = 11.0622; jz = 0.
        // Круговой инвариант: forward(joints) восстанавливает TCP-позу
        // каждого хода (см. 12.5.3-oracle-тест ниже).
        let program = L2Program::new(
            vec![
                assign(100, Expr::num(15.0)),
                assign(
                    101,
                    Expr::Binary(
                        BinaryOp::Mul,
                        Box::new(Expr::var(100)),
                        Box::new(Expr::num(2.0)),
                    ),
                ),
                move_stmt(
                    MoveKind::Linear,
                    ExprPose {
                        x: Some(Expr::num(10.0)),
                        y: None,
                        z: None,
                        a: None,
                        b: None,
                        c: Some(Expr::var(101)),
                    },
                    Some(Expr::num(300.0)),
                ),
                move_stmt(
                    MoveKind::Rapid,
                    ExprPose {
                        x: None,
                        y: Some(Expr::num(7.0)),
                        z: None,
                        a: None,
                        b: None,
                        c: None,
                    },
                    None,
                ),
            ],
            Vec::new(),
        );
        let mk = |rtcp: RtcpMode| L2Config {
            rtcp_mode: rtcp,
            initial_pos: Some([0.0, 0.0, 0.0]), // позиция известна — трекинг с кадра 1
            ..cfg()
        };
        let trt = L2Evaluator::new(mk(RtcpMode::XyzacTrt {
            params: TrtParams::default(),
        }))
        .run(&program)
        .unwrap();
        assert!(trt.rtcp_active);
        assert_eq!(trt.l1_ops.len(), 2); // ОБА хода lowered (полный-TCP 12.5.3)
        assert_eq!(trt.raw_tail.len(), 0); // носителей нет — всё опущено
        if let Op::Linear { end, state, .. } = &trt.l1_ops[0] {
            let j = (
                end.x.value(),
                end.y.value(),
                end.z.value(),
                state.feed.as_ref().map(|f| f.value()),
            );
            let approx = |a: f64, b: f64| (a - b).abs() < 5e-5; // 4dp-канон
            assert!(approx(j.0, 8.6603), "jx = {j:?}");
            assert!(approx(j.1, 5.0), "jy = {j:?}");
            assert!(approx(j.2, 0.0), "jz = {j:?}");
            assert_eq!(j.3, Some(300.0)); // feed пронаследован lowering'ом
        } else {
            panic!("expected lowered Linear");
        }
        if let Op::Rapid { end, .. } = &trt.l1_ops[1] {
            let j = (end.x.value(), end.y.value(), end.z.value());
            let approx = |a: f64, b: f64| (a - b).abs() < 5e-5;
            assert!(approx(j.0, 5.1603), "jx = {j:?}");
            assert!(approx(j.1, 11.0622), "jy = {j:?}");
            assert!(approx(j.2, 0.0), "jz = {j:?}");
        } else {
            panic!("expected lowered Rapid");
        }
        // Off-режим (MachineKinematics::Triv): «Raw как прежде» — ротационный
        // кадр носитель, линейный кадр TCP-оп (12.2-семантика).
        let off = L2Evaluator::new(mk(RtcpMode::Off)).run(&program).unwrap();
        assert!(!off.rtcp_active);
        assert_eq!(off.l1_ops.len(), 1);
        assert_eq!(off.raw_tail.len(), 1);
        assert_eq!(off.raw_tail[0].marker, RawMarker::RotaryAxis);
        assert_eq!(
            decode_payload(&off.raw_tail[0].op),
            "G1 X10.0000 C30.0000 F300.0000\n"
        );
        if let Op::Rapid { end, .. } = &off.l1_ops[0] {
            // TCP-координаты (не joints): наследование X=10 из носителя.
            assert_eq!(
                (end.x.value(), end.y.value(), end.z.value()),
                (10.0, 7.0, 0.0)
            );
        } else {
            panic!("expected TCP Rapid");
        }
    }

    #[test]
    fn t8_rtcp_trt_without_rotary_words_full_tcp_identity_at_zero_params() {
        // 12.5.3: под trt-профилем ЛЮБОЙ ход с полной TCP-позой опускается
        // joints-опом (полный-TCP режим). При НУЛЕВОЙ геометрии и углах
        // home (0,0) inverse тождественен (A=C=0: круговой инвариант
        // вырождается в identity) — L1-опы численно равны Off; НО флаг
        // rtcp_active и per-op диагностика честно ставятся: поток —
        // machine-оси (сдвиг пина 12.5.2 «маркировка только на ротационных»:
        // при ненулевой геометрии линейные кадры тоже пересчитываются —
        // «не молча» обязан покрывать весь trt-поток).
        let program = L2Program::new(
            vec![move_stmt(
                MoveKind::Rapid,
                target_const(1.0, 2.0, 3.0),
                None,
            )],
            Vec::new(),
        );
        let off = ev().run(&program).unwrap();
        let trt = L2Evaluator::new(cfg_trt()).run(&program).unwrap();
        assert_eq!(off.l1_ops, trt.l1_ops); // нулевые параметры: identity
        assert_eq!(off.raw_tail, trt.raw_tail);
        assert!(!off.rtcp_active);
        assert!(trt.rtcp_active);
        // per-op диагностика называет профиль и семантику machine-осей.
        assert!(trt
            .diagnostics
            .iter()
            .any(|d| d.message.contains("XyzacTrt") && d.message.contains("machine-axis joints")));
    }

    // ---------- 12.5.4: нелинейные топологии (XyzabTdr/Trsrn/Maxkins/FiveAxis) —

    fn cfg_tdr() -> L2Config {
        L2Config {
            rtcp_mode: RtcpMode::XyzabTdr {
                params: crate::kinematics_tdr::TdrParams::new([0.0; 3], 0.0, 0.0, 0.0),
            },
            ..cfg()
        }
    }

    fn cfg_trsrn(nut: f64) -> L2Config {
        L2Config {
            rtcp_mode: RtcpMode::Trsrn {
                params: crate::kinematics_trsrn::TrsrnParams {
                    nut_angle: nut,
                    ..Default::default()
                },
            },
            ..cfg()
        }
    }

    fn cfg_max() -> L2Config {
        L2Config {
            rtcp_mode: RtcpMode::Maxkins {
                params: crate::kinematics_maxkins::MaxkinsParams::default(),
            },
            ..cfg()
        }
    }

    fn cfg_5axis() -> L2Config {
        L2Config {
            rtcp_mode: RtcpMode::FiveAxis {
                params: crate::kinematics_5axis::FiveAxisParams::default(),
            },
            ..cfg()
        }
    }

    fn full_tcp_linear(x: f64, y: f64, z: f64) -> L2Program {
        L2Program::new(
            vec![move_stmt(
                MoveKind::Linear,
                ExprPose {
                    x: Some(Expr::num(x)),
                    y: Some(Expr::num(y)),
                    z: Some(Expr::num(z)),
                    a: None,
                    b: None,
                    c: None,
                },
                Some(Expr::num(300.0)),
            )],
            Vec::new(),
        )
    }

    #[test]
    fn v1254_tdr_full_tcp_linear_lowers_to_joints() {
        // Полный-TCP: XyzabTdr — linear (10,20,30) A30 B0 при нулевой
        // геометрии: TCP → tdr_inverse → machine-оси; круговой инвариант
        // ядром: forward(inverse(TCP)) == TCP.
        let program = L2Program::new(
            vec![move_stmt(
                MoveKind::Linear,
                ExprPose {
                    x: Some(Expr::num(10.0)),
                    y: Some(Expr::num(20.0)),
                    z: Some(Expr::num(30.0)),
                    a: Some(Expr::num(30.0)),
                    b: Some(Expr::num(0.0)),
                    c: None,
                },
                Some(Expr::num(300.0)),
            )],
            Vec::new(),
        );
        let res = L2Evaluator::new(cfg_tdr()).run(&program).unwrap();
        assert!(res.rtcp_active);
        assert_eq!(res.l1_ops.len(), 1, "полный-TCP: linear lowered");
        if let Op::Linear { end, .. } = &res.l1_ops[0] {
            let j = crate::kinematics_tdr::JointsTdr::new(
                end.x.value(),
                end.y.value(),
                end.z.value(),
                30.0,
                0.0,
            );
            let back = crate::kinematics_tdr::tdr_forward(
                &crate::kinematics_tdr::TdrParams::new([0.0; 3], 0.0, 0.0, 0.0),
                &j,
            );
            // Круг: machine-оси → forward → исходный TCP (10,20,30).
            assert!((back.x - 10.0).abs() < 5e-5, "back.x = {}", back.x);
            assert!((back.y - 20.0).abs() < 5e-5, "back.y = {}", back.y);
            assert!((back.z - 30.0).abs() < 5e-5, "back.z = {}", back.z);
        } else {
            panic!("expected lowered Linear");
        }
    }

    #[test]
    fn v1254_trsrn_solver_path_lowers_orientation_to_joints() {
        // Trsrn — SOLVER-путь (НЕ pass-through): программные углы B/C =
        // ориентация инструмента; machine joints B/C решаются
        // trsrn_angles_from_tool_axis; линейная TCP → trsrn_tcp_inverse.
        let program = L2Program::new(
            vec![move_stmt(
                MoveKind::Linear,
                ExprPose {
                    x: Some(Expr::num(10.0)),
                    y: Some(Expr::num(0.0)),
                    z: Some(Expr::num(20.0)),
                    a: None,
                    b: Some(Expr::num(45.0)),
                    c: Some(Expr::num(90.0)),
                },
                Some(Expr::num(300.0)),
            )],
            Vec::new(),
        );
        let res = L2Evaluator::new(cfg_trsrn(90.0)).run(&program).unwrap();
        assert!(res.rtcp_active);
        assert!(!res.l1_ops.is_empty());
        // «Не молча»: per-op диагностика называет профиль Trsrn.
        assert!(res.diagnostics.iter().any(|d| d.message.contains("Trsrn")));
    }

    #[test]
    fn v1254_maxkins_and_fiveaxis_full_tcp_lowers_linear() {
        // Полный-TCP скоуп: X/Y/Z-кадр (без ротационных слов) под
        // Maxkins/FiveAxis опускается L1-опом machine-осей (TCP → inverse);
        // rtcp_active + per-op диагностика называет профиль.
        for cfg in [cfg_max(), cfg_5axis()] {
            let name = cfg.rtcp_mode.name().to_string();
            let program = full_tcp_linear(5.0, 6.0, 7.0);
            let res = L2Evaluator::new(cfg).run(&program).unwrap();
            assert!(res.rtcp_active, "{name}: rtcp_active");
            assert_eq!(res.l1_ops.len(), 1, "{name}: full TCP lowered");
            assert!(
                res.diagnostics
                    .iter()
                    .any(|d| d.message.contains(&name) && d.message.contains("machine axes")),
                "{name}: not silent"
            );
        }
    }

    #[test]
    fn v1254_arc_under_kinematics_profile_is_honest_carrier() {
        // Дуга под kinematics-профилем → честный носитель (TCP-дуга не
        // аффинно отображается в machine-оси; 5-осевая интерполяция —
        // не аффинная геометрия — граница 12.5.3 сохраняется для всех).
        let program = L2Program::new(
            vec![
                // Предварительный linear: позиция известна (иначе arc уходит
                // в «start position unknown»-носитель до kinematics-гейта).
                move_stmt(
                    MoveKind::Linear,
                    ExprPose {
                        x: Some(Expr::num(0.0)),
                        y: Some(Expr::num(0.0)),
                        z: Some(Expr::num(5.0)),
                        a: None,
                        b: None,
                        c: None,
                    },
                    Some(Expr::num(300.0)),
                ),
                move_stmt(
                    MoveKind::Arc {
                        ijk: [Some(Expr::num(5.0)), Some(Expr::num(0.0)), None],
                        r: None,
                        dir: Dir::Cw,
                    },
                    ExprPose {
                        x: Some(Expr::num(10.0)),
                        y: Some(Expr::num(0.0)),
                        z: Some(Expr::num(0.0)),
                        a: None,
                        b: None,
                        c: None,
                    },
                    Some(Expr::num(300.0)),
                ),
            ],
            Vec::new(),
        );
        for cfg in [cfg_tdr(), cfg_trsrn(90.0), cfg_max(), cfg_5axis()] {
            let name = cfg.rtcp_mode.name().to_string();
            let res = L2Evaluator::new(cfg).run(&program).unwrap();
            assert!(res.rtcp_active, "{name}: arc carrier rtcp_active");
            // Дуга = носитель; positioning-linear (кадр 1) — единственный
            // L1-оп (дуга НЕ добавляет lowering).
            assert_eq!(res.l1_ops.len(), 1, "{name}: arc must stay carrier");
        }
    }

    #[test]
    fn v1254_off_mode_stays_pure_carrier() {
        // Анти-пин: Off — НИКАКОЙ rtcp-маркировки.
        let program = full_tcp_linear(1.0, 2.0, 3.0);
        let res = L2Evaluator::new(cfg()).run(&program).unwrap();
        assert!(!res.rtcp_active);
    }

    // ---------- 12.5.3: TCP→joints trt-пересчёт (MachineKinematics) ----------

    use crate::kinematics::{xyzac_forward, xyzbc_forward};

    /// Ручной golden (бриф 12.5.3): TCP-поза (15, 10, 5), A=90°, C=0°,
    /// геометрия без офсетов (TrtParams::default, con=−1) → joints через
    /// xyzac_inverse: (15, −5, 10); круговой инвариант: forward(joints)
    /// восстанавливает pose. Полные слова кадра — lowering без initial_pos.
    #[test]
    fn t1253_golden_tcp_a90c0_to_joints_and_back() {
        let program = L2Program::new(
            vec![move_stmt(
                MoveKind::Rapid,
                ExprPose {
                    x: Some(Expr::num(15.0)),
                    y: Some(Expr::num(10.0)),
                    z: Some(Expr::num(5.0)),
                    a: Some(Expr::num(90.0)),
                    b: None,
                    c: Some(Expr::num(0.0)),
                },
                None,
            )],
            Vec::new(),
        );
        let res = L2Evaluator::new(cfg_trt()).run(&program).unwrap();
        assert!(res.rtcp_active);
        assert!(res.raw_tail.is_empty());
        assert_eq!(res.l1_ops.len(), 1);
        let Op::Rapid { end, .. } = &res.l1_ops[0] else {
            panic!("expected Rapid");
        };
        // Ручной расчёт (con=−1, без офсетов): x = cos0·15 = 15;
        // y = −(−1)·sin0·cos90·15 + cos0·cos90·10 + (−1)·sin90·5 = −5;
        // z = sin0·sin90·15 − (−1)·cos0·sin90·10 + cos90·5 = 10.
        let approx = |a: f64, b: f64| (a - b).abs() < 5e-5; // 4dp-канон op
        assert!(approx(end.x.value(), 15.0));
        assert!(approx(end.y.value(), -5.0), "jy = {}", end.y.value());
        assert!(approx(end.z.value(), 10.0), "jz = {}", end.z.value());
        // Оракул-круг: forward(inverse(pose)) == pose (kinematics-ядро).
        let fwd = xyzac_forward(
            &crate::kinematics::TrtParams::default(),
            &crate::kinematics::Joints5::new(
                end.x.value(),
                end.y.value(),
                end.z.value(),
                90.0,
                0.0,
            ),
        );
        for (got, want) in [(fwd.x, 15.0), (fwd.y, 10.0), (fwd.z, 5.0)] {
            assert!((got - want).abs() < 1e-3, "forward {got} != {want}");
        }
        // per-op диагностика называет профиль/фазу («не молча»).
        assert!(res.diagnostics.iter().any(|d| {
            d.message.contains("12.5.3") && d.message.contains("machine-axis joints")
        }));
    }

    /// Оракул-решётка (100+ мутаций): pose → lowering (inverse в joints-оп,
    /// 4dp-канон) → forward → сравнение с pose (круговой инвариант через
    /// lowering). Углы A×C, позиции, оба флага con — 200 кейсов.
    #[test]
    fn t1253_oracle_circle_lattice_100plus_cases() {
        let angles = [0.0, 30.0, 90.0, 180.0, 359.0];
        let cs = [0.0, 45.0, -90.0, 359.0];
        let positions = [
            [15.0, 10.0, 5.0],
            [100.0, -50.0, 25.0],
            [0.5, -0.25, 0.75],
            [0.0, 0.0, 0.0],
            [-200.0, 300.0, -400.0],
        ];
        let mut cases = 0usize;
        for &conventional in &[false, true] {
            let params =
                crate::kinematics::TrtParams::new([0.0, 0.0, 0.0], 0.0, 0.0, 0.0, conventional);
            for &a in &angles {
                for &c in &cs {
                    for pos in &positions {
                        let program = L2Program::new(
                            vec![move_stmt(
                                MoveKind::Linear,
                                ExprPose {
                                    x: Some(Expr::num(pos[0])),
                                    y: Some(Expr::num(pos[1])),
                                    z: Some(Expr::num(pos[2])),
                                    a: Some(Expr::num(a)),
                                    b: None,
                                    c: Some(Expr::num(c)),
                                },
                                None,
                            )],
                            Vec::new(),
                        );
                        let res = L2Evaluator::new(L2Config {
                            rtcp_mode: RtcpMode::XyzacTrt { params },
                            ..cfg()
                        })
                        .run(&program)
                        .unwrap();
                        assert!(res.raw_tail.is_empty(), "case must lower: a={a} c={c}");
                        let Op::Linear { end, .. } = &res.l1_ops[0] else {
                            panic!("expected Linear");
                        };
                        // joints (4dp-канон op) → forward → pose.
                        let j = crate::kinematics::Joints5::new(
                            end.x.value(),
                            end.y.value(),
                            end.z.value(),
                            a,
                            c,
                        );
                        let fwd = xyzac_forward(&params, &j);
                        let approx =
                            |p: f64, q: f64| (p - q).abs() < (1e-3f64).max(1e-9 * q.abs() * 2.0);
                        assert!(
                            approx(fwd.x, pos[0]),
                            "con={conventional} a={a} c={c} pos={pos:?}: fwd.x={} != {}",
                            fwd.x,
                            pos[0]
                        );
                        assert!(
                            approx(fwd.y, pos[1]),
                            "con={conventional} a={a} c={c} pos={pos:?}: fwd.y={} != {}",
                            fwd.y,
                            pos[1]
                        );
                        assert!(
                            approx(fwd.z, pos[2]),
                            "con={conventional} a={a} c={c} pos={pos:?}: fwd.z={} != {}",
                            fwd.z,
                            pos[2]
                        );
                        cases += 1;
                    }
                }
            }
        }
        assert!(cases >= 100, "решётка мала: {cases}");
    }

    /// Чужое ротационное слово: B под XYZAC и A под XYZBC → честный носитель
    /// (топология профиля не моделирует ось; head-head и др. — при формулах).
    #[test]
    fn t1253_foreign_rotary_word_carrier_both_topologies() {
        let mk = |family: RtcpMode| L2Config {
            rtcp_mode: family,
            initial_pos: Some([0.0, 0.0, 0.0]),
            ..cfg()
        };
        // B-слово под XyzacTrt (позиция известна — ошибка НЕ в позе).
        let program = L2Program::new(
            vec![move_stmt(
                MoveKind::Rapid,
                ExprPose {
                    x: Some(Expr::num(1.0)),
                    y: Some(Expr::num(2.0)),
                    z: Some(Expr::num(3.0)),
                    a: None,
                    b: Some(Expr::num(45.0)),
                    c: None,
                },
                None,
            )],
            Vec::new(),
        );
        let res = L2Evaluator::new(mk(RtcpMode::XyzacTrt {
            params: crate::kinematics::TrtParams::default(),
        }))
        .run(&program)
        .unwrap();
        assert!(res.l1_ops.is_empty());
        assert_eq!(res.raw_tail[0].marker, RawMarker::RotaryAxis);
        assert!(res.diagnostics.iter().any(|d| {
            d.message
                .contains("outside the MachineKinematics XyzacTrt topology")
        }));
        // A-слово под XyzbcTrt.
        let program_b = L2Program::new(
            vec![move_stmt(
                MoveKind::Rapid,
                ExprPose {
                    x: Some(Expr::num(1.0)),
                    y: Some(Expr::num(2.0)),
                    z: Some(Expr::num(3.0)),
                    a: Some(Expr::num(45.0)),
                    b: None,
                    c: None,
                },
                None,
            )],
            Vec::new(),
        );
        let res_b = L2Evaluator::new(mk(RtcpMode::XyzbcTrt {
            params: crate::kinematics::TrtParams::default(),
        }))
        .run(&program_b)
        .unwrap();
        assert!(res_b.l1_ops.is_empty());
        assert_eq!(res_b.raw_tail[0].marker, RawMarker::RotaryAxis);
        assert!(res_b.diagnostics.iter().any(|d| {
            d.message
                .contains("outside the MachineKinematics XyzbcTrt topology")
        }));
    }

    /// Модальность ротационных осей (трекинг углов): A задан кадром 1,
    /// кадр 2 без A — joints считаются на УНАСЛЕДОВАННОМ угле A=90.
    #[test]
    fn t1253_rot_angle_inherited_across_frames() {
        let program = L2Program::new(
            vec![
                move_stmt(
                    MoveKind::Rapid,
                    ExprPose {
                        x: Some(Expr::num(0.0)),
                        y: Some(Expr::num(0.0)),
                        z: Some(Expr::num(0.0)),
                        a: Some(Expr::num(90.0)),
                        b: None,
                        c: Some(Expr::num(0.0)),
                    },
                    None,
                ),
                move_stmt(
                    MoveKind::Linear,
                    ExprPose {
                        x: None,
                        y: Some(Expr::num(7.0)),
                        z: None,
                        a: None,
                        b: None,
                        c: None,
                    },
                    None,
                ),
            ],
            Vec::new(),
        );
        let res = L2Evaluator::new(cfg_trt()).run(&program).unwrap();
        assert!(res.rtcp_active);
        assert_eq!(res.l1_ops.len(), 2);
        // Кадр 2: TCP (0, 7, 0) на углах (A=90, C=0) → joints (0, 0, 7):
        // y_j = cos90·7·... ручной расчёт (con=−1): x = 0; y = cos90·7 = 0...
        // по формулам inverse: y = −con·sc·ca·x + cc·ca·y + con·sa·z = 0
        // (ca=0, sc=0); z = sc·sa·x − con·cc·sa·y + ca·z = 1·1·7 = 7.
        let Op::Linear { end, .. } = &res.l1_ops[1] else {
            panic!("expected Linear");
        };
        let approx = |p: f64, q: f64| (p - q).abs() < 5e-5;
        assert!(approx(end.x.value(), 0.0));
        assert!(approx(end.y.value(), 0.0));
        assert!(approx(end.z.value(), 7.0), "jz = {}", end.z.value());
        // Оракул: forward((0,0,7), A90, C0) == TCP (0,7,0).
        let fwd = xyzac_forward(
            &crate::kinematics::TrtParams::default(),
            &crate::kinematics::Joints5::new(0.0, 0.0, 7.0, 90.0, 0.0),
        );
        assert!((fwd.y - 7.0).abs() < 1e-9 && fwd.z.abs() < 1e-9);
    }

    /// XYZBC-путь: golden-якорь kinematics-ядра (dx=10, B=90, C=0,
    /// joints (100,200,300) → pose (−290,200,90)) через lowering:
    /// TCP-кадр → joints-оп; круг forward замыкается.
    #[test]
    fn t1253_xyzbc_profile_path_golden() {
        let program = L2Program::new(
            vec![move_stmt(
                MoveKind::Linear,
                ExprPose {
                    x: Some(Expr::num(-290.0)),
                    y: Some(Expr::num(200.0)),
                    z: Some(Expr::num(90.0)),
                    a: None,
                    b: Some(Expr::num(90.0)),
                    c: Some(Expr::num(0.0)),
                },
                None,
            )],
            Vec::new(),
        );
        let params = crate::kinematics::TrtParams::new([0.0; 3], 0.0, 10.0, 0.0, false);
        let res = L2Evaluator::new(L2Config {
            rtcp_mode: RtcpMode::XyzbcTrt { params },
            ..cfg()
        })
        .run(&program)
        .unwrap();
        assert!(res.rtcp_active);
        assert!(res.raw_tail.is_empty());
        let Op::Linear { end, .. } = &res.l1_ops[0] else {
            panic!("expected Linear");
        };
        let approx = |p: f64, q: f64| (p - q).abs() < 5e-5;
        assert!(approx(end.x.value(), 100.0), "jx = {}", end.x.value());
        assert!(approx(end.y.value(), 200.0), "jy = {}", end.y.value());
        assert!(approx(end.z.value(), 300.0), "jz = {}", end.z.value());
        // Круг: forward(joints) == pose.
        let fwd = xyzbc_forward(
            &params,
            &crate::kinematics::JointsBC::new(
                end.x.value(),
                end.y.value(),
                end.z.value(),
                90.0,
                0.0,
            ),
        );
        assert!((fwd.x + 290.0).abs() < 1e-3);
        assert!((fwd.y - 200.0).abs() < 1e-3);
        assert!((fwd.z - 90.0).abs() < 1e-3);
    }

    /// Дуга под trt-профилем → честный носитель (TCP-дуга не аффинна в
    /// machine-осях); Off — дуга опускается TCP-опом как прежде.
    #[test]
    fn t1253_arc_under_trt_is_honest_carrier() {
        let arc_stmt = L2Stmt::Move {
            kind: MoveKind::Arc {
                ijk: [Some(Expr::num(-5.0)), Some(Expr::num(0.0)), None],
                r: None,
                dir: eid_core::ast::Dir::Ccw,
            },
            target: ExprPose {
                x: Some(Expr::num(5.0)),
                y: Some(Expr::num(5.0)),
                z: Some(Expr::num(2.0)),
                a: None,
                b: None,
                c: None,
            },
            feed: None,
            spindle: None,
            src: placeholder_span(),
        };
        let program = L2Program::new(vec![arc_stmt], Vec::new());
        let mk = |rtcp: RtcpMode| L2Config {
            rtcp_mode: rtcp,
            initial_pos: Some([10.0, 0.0, 2.0]), // старт дуги известен
            ..cfg()
        };
        // Trt: носитель + rtcp_active.
        let trt = L2Evaluator::new(mk(RtcpMode::XyzacTrt {
            params: crate::kinematics::TrtParams::default(),
        }))
        .run(&program)
        .unwrap();
        assert!(trt.rtcp_active);
        assert!(trt.l1_ops.is_empty());
        assert_eq!(trt.raw_tail[0].marker, RawMarker::ArcUnlowered);
        assert!(trt.diagnostics.iter().any(|d| {
            d.message
                .contains("not affinely representable in machine-axis moves")
        }));
        // Off: дуга lowered (TCP-семантика) — прежнее поведение.
        let off = L2Evaluator::new(mk(RtcpMode::Off)).run(&program).unwrap();
        assert!(!off.rtcp_active);
        assert_eq!(off.l1_ops.len(), 1);
        assert!(matches!(off.l1_ops[0], Op::Arc { .. }));
    }

    // ---------- D1: кадры, глубина 9, M99/Return, G66 ----------

    #[test]
    fn d1_call_sub_args_and_frames() {
        // Main: G65 P100 A5. B7. → sub O100: #100 = #1 + #2 (локальные!).
        let program = L2Program::new(
            vec![call(
                SubTarget::ONumber(100),
                vec![(b'A', Expr::num(5.0)), (b'b', Expr::num(7.0))], // lowercase → #2
            )],
            vec![crate::ast::L2Sub {
                target: SubTarget::ONumber(100),
                body: vec![
                    assign(
                        100,
                        Expr::Binary(
                            BinaryOp::Add,
                            Box::new(Expr::var(1)),
                            Box::new(Expr::var(2)),
                        ),
                    ),
                    assign(1, Expr::num(0.0)), // запись в локальную — свой кадр
                ],
            }],
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(100).unwrap(), Val::Num(12.0));
        // Стек вернулся к корню; локальная #1 главного блока не тронута.
        assert_eq!(e.env().frame_depth(), 1);
        assert_eq!(e.env().read_direct(1).unwrap(), Val::Null);
    }

    #[test]
    fn d1_pin_call_sub_null_arg_passes_through() {
        // ПИН (таблица 2.1, строка 4): «G65 P1000 X#0 — внутри макроса #24
        // получает Val::Null». Контраст: X0.0 → Num(0.0).
        let mk = |x_val: Expr| {
            L2Program::new(
                vec![call(SubTarget::ONumber(100), vec![(b'X', x_val)])],
                vec![crate::ast::L2Sub {
                    target: SubTarget::ONumber(100),
                    body: vec![assign(500, Expr::var(24))],
                }],
            )
        };
        let mut e = ev();
        e.run(&mk(Expr::Null)).unwrap();
        assert_eq!(e.env().read_direct(500).unwrap(), Val::Null);
        let mut e2 = ev();
        e2.run(&mk(Expr::num(0.0))).unwrap();
        assert_eq!(e2.env().read_direct(500).unwrap(), Val::Num(0.0));
    }

    #[test]
    fn d1_call_depth_limit_9() {
        // Дизайн §5 12.2: глубина стека вызовов до 9; 10-й вложенный вызов
        // → E_LIMIT_EXCEEDED ([L2ExecError::CallDepthExceeded]).
        let sub_body = vec![
            assign(
                500,
                Expr::Binary(
                    BinaryOp::Add,
                    Box::new(Expr::var(500)),
                    Box::new(Expr::num(1.0)),
                ),
            ),
            call(SubTarget::ONumber(900), vec![]),
        ];
        let program = L2Program::new(
            vec![call(SubTarget::ONumber(900), vec![])],
            vec![crate::ast::L2Sub {
                target: SubTarget::ONumber(900),
                body: sub_body,
            }],
        );
        let mut e = ev(); // max_call_depth = 9
        let err = e.run(&program).unwrap_err();
        assert_eq!(err, L2ExecError::CallDepthExceeded { limit: 9 });
        assert_eq!(err.error_code(), ErrorCode::LimitExceeded);
        // 9 АКТИВНЫХ вызовов допустимы: счётчик дошёл до 9 до падения.
        assert_eq!(e.env().read_direct(500).unwrap(), Val::Num(9.0));
        // Расширенный лимит падает позже (на общем бюджете — не сразу).
        let mut deep = L2Evaluator::new(L2Config {
            max_call_depth: 12,
            max_eval_steps: 500,
            ..cfg()
        });
        let err = deep.run(&program).unwrap_err();
        assert_eq!(err, L2ExecError::CallDepthExceeded { limit: 12 });
    }

    #[test]
    fn d1_m99_return_pop_and_implicit_return() {
        // M99 ДО конца тела: операторы после Return не исполняются;
        // О-конец без M99 — неявный return (оба пути: pop кадра).
        let mk = |body: Vec<L2Stmt>| {
            L2Program::new(
                vec![call(SubTarget::ONumber(100), vec![])],
                vec![crate::ast::L2Sub {
                    target: SubTarget::ONumber(100),
                    body,
                }],
            )
        };
        let program = mk(vec![
            assign(
                100,
                Expr::Binary(
                    BinaryOp::Add,
                    Box::new(Expr::var(100)),
                    Box::new(Expr::num(1.0)),
                ),
            ),
            L2Stmt::Return {
                src: placeholder_span(),
            },
            assign(
                100,
                Expr::Binary(
                    BinaryOp::Add,
                    Box::new(Expr::var(100)),
                    Box::new(Expr::num(10.0)),
                ),
            ),
        ]);
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(100).unwrap(), Val::Num(1.0));
        assert_eq!(e.env().frame_depth(), 1, "M99 pops the frame");
        // Неявный return.
        let program = mk(vec![assign(
            100,
            Expr::Binary(
                BinaryOp::Add,
                Box::new(Expr::var(100)),
                Box::new(Expr::num(100.0)),
            ),
        )]);
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(100).unwrap(), Val::Num(100.0));
        assert_eq!(e.env().frame_depth(), 1);
        // M99 в корне — ошибка (нет активного вызова).
        let root = L2Program::new(
            vec![L2Stmt::Return {
                src: placeholder_span(),
            }],
            Vec::new(),
        );
        let err = ev().run(&root).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::RuntimeState);
    }

    #[test]
    fn d1_m99_inside_while_returns_early() {
        // M99 внутри тела цикла: ранний выход из подпрограммы (Return
        // пузырится через While до call-site).
        let program = L2Program::new(
            vec![call(SubTarget::ONumber(100), vec![])],
            vec![crate::ast::L2Sub {
                target: SubTarget::ONumber(100),
                body: vec![
                    L2Stmt::While {
                        cond: Expr::num(1.0),
                        do_id: 1,
                        body: vec![L2Stmt::Return {
                            src: placeholder_span(),
                        }],
                        src: placeholder_span(),
                    },
                    assign(100, Expr::num(777.0)), // не исполняется
                ],
            }],
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(100).unwrap(), Val::Null);
        assert_eq!(e.env().frame_depth(), 1);
    }

    #[test]
    fn d1_repeat_l_unroll() {
        // repeat = L2: подпрограмма O200 дважды инкрементит глобальную #500.
        let mk_call = |r: Option<Expr>| L2Stmt::CallSub {
            target: SubTarget::ONumber(200),
            repeat: r,
            args: vec![],
            modal: false,
            src: placeholder_span(),
        };
        let program = L2Program::new(
            vec![mk_call(Some(Expr::num(2.0)))],
            vec![crate::ast::L2Sub {
                target: SubTarget::ONumber(200),
                body: vec![assign(
                    500,
                    Expr::Binary(
                        BinaryOp::Add,
                        Box::new(Expr::var(500)),
                        Box::new(Expr::num(1.0)),
                    ),
                )],
            }],
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(500).unwrap(), Val::Num(2.0));
        // Неизвестная цель → RuntimeError (E_RUNTIME_STATE класс, 12.1-пин).
        let bad = L2Program::new(vec![mk_call(None)], Vec::new());
        let err = ev().run(&bad).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::RuntimeState);
    }

    #[test]
    fn d1_modal_g66_skeleton_trigger_after_motion() {
        // G66-каркас: регистрация (носитель SubCall), затем КАЖДЫЙ lowered
        // motion триггерит тело (два хода → #500 = 2). G67 не имеет узла
        // AST — контракт-пробел (шапка модуля).
        let program = L2Program::new(
            vec![
                L2Stmt::CallSub {
                    target: SubTarget::ONumber(300),
                    repeat: None,
                    args: vec![],
                    modal: true,
                    src: placeholder_span(),
                },
                move_stmt(MoveKind::Rapid, target_const(1.0, 0.0, 0.0), None),
                move_stmt(MoveKind::Rapid, target_const(2.0, 0.0, 0.0), None),
            ],
            vec![crate::ast::L2Sub {
                target: SubTarget::ONumber(300),
                body: vec![assign(
                    500,
                    Expr::Binary(
                        BinaryOp::Add,
                        Box::new(Expr::var(500)),
                        Box::new(Expr::num(1.0)),
                    ),
                )],
            }],
        );
        let mut e = ev();
        let res = e.run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 2);
        assert!(res.raw_tail.iter().any(|c| c.marker == RawMarker::SubCall));
        assert_eq!(e.env().read_direct(500).unwrap(), Val::Num(2.0));
    }

    // ---------- D2: CFG IfGoto/таблицы меток, бюджеты ----------

    #[test]
    fn d2_if_goto_cfg_jump_backward_loop() {
        // N1: #500 += 1; IF [#500 LT 3] GOTO 1 → три итерации, #500 = 3.
        let program = L2Program::new(
            vec![
                label(1),
                assign(
                    500,
                    Expr::Binary(
                        BinaryOp::Add,
                        Box::new(Expr::var(500)),
                        Box::new(Expr::num(1.0)),
                    ),
                ),
                goto(
                    1,
                    Expr::Binary(
                        BinaryOp::Lt,
                        Box::new(Expr::var(500)),
                        Box::new(Expr::num(3.0)),
                    ),
                ),
            ],
            Vec::new(),
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(500).unwrap(), Val::Num(3.0));
    }

    #[test]
    fn d2_if_goto_jump_forward_skips_dead_code() {
        // IF [1 EQ 1] GOTO 20 (D3-тривиальное условие) — переход через
        // мёртвый код; N20 — через узлы Label.
        let program = L2Program::new(
            vec![
                goto(
                    20,
                    Expr::Binary(
                        BinaryOp::Eq,
                        Box::new(Expr::num(1.0)),
                        Box::new(Expr::num(1.0)),
                    ),
                ),
                assign(500, Expr::num(666.0)), // мёртвый код
                label(20),
                assign(501, Expr::num(1.0)),
            ],
            Vec::new(),
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(500).unwrap(), Val::Null);
        assert_eq!(e.env().read_direct(501).unwrap(), Val::Num(1.0));
    }

    #[test]
    fn d2_parser_label_table_contract() {
        // КОНТРАКТ (а) для B2/D2: таблица в L2Program.labels (парсер) —
        // без узлов Label: GOTO 5 → stmt-индекс 3 (парсерская таблица
        // имеет приоритет над собранной по узлам).
        let with_goto = L2Program {
            stmts: vec![
                goto(5, Expr::num(1.0)),
                assign(500, Expr::num(99.0)), // пропущено
                assign(500, Expr::num(88.0)), // пропущено
                assign(500, Expr::num(3.0)),  // N5 = stmt 3 (по таблице)
            ],
            subs: Vec::new(),
            labels: {
                let mut m = BTreeMap::new();
                m.insert(5u32, 3usize);
                m
            },
        };
        let mut e = ev();
        e.run(&with_goto).unwrap();
        assert_eq!(e.env().read_direct(500).unwrap(), Val::Num(3.0));
    }

    #[test]
    fn d2_goto_unknown_label_is_error() {
        let program = L2Program::new(vec![goto(42, Expr::num(1.0))], Vec::new());
        let err = ev().run(&program).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::RuntimeState);
        assert!(err.message().contains("N42"));
    }

    #[test]
    fn d2_labels_are_body_scoped_in_subs() {
        // Метки тела подпрограммы — СВОИ: GOTO 5 в main не видит метку sub.
        let subs = vec![crate::ast::L2Sub {
            target: SubTarget::ONumber(100),
            body: vec![label(5), assign(500, Expr::num(1.0))],
        }];
        let with_goto_main = L2Program {
            stmts: vec![
                goto(5, Expr::num(1.0)),
                call(SubTarget::ONumber(100), vec![]),
            ],
            subs,
            labels: BTreeMap::new(),
        };
        let err = ev().run(&with_goto_main).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::RuntimeState);
        // А внутри sub — работает (узлы Label).
        let with_goto_sub = L2Program {
            stmts: vec![call(SubTarget::ONumber(100), vec![])],
            subs: vec![crate::ast::L2Sub {
                target: SubTarget::ONumber(100),
                body: vec![
                    goto(5, Expr::num(1.0)),
                    assign(500, Expr::num(99.0)),
                    label(5),
                    assign(500, Expr::num(1.0)),
                ],
            }],
            labels: BTreeMap::new(),
        };
        let mut e = ev();
        e.run(&with_goto_sub).unwrap();
        assert_eq!(e.env().read_direct(500).unwrap(), Val::Num(1.0));
    }

    #[test]
    fn t5_infinite_while_limit_exceeded() {
        // #100 = 1; WHILE [#100 GT 0] DO1 END1 — бесконечный цикл.
        // T5: детерминированное прерывание, класс E_LIMIT_EXCEEDED.
        let program = L2Program::new(
            vec![
                assign(100, Expr::num(1.0)),
                L2Stmt::While {
                    cond: Expr::Binary(
                        BinaryOp::Gt,
                        Box::new(Expr::var(100)),
                        Box::new(Expr::num(0.0)),
                    ),
                    do_id: 1,
                    body: vec![],
                    src: placeholder_span(),
                },
            ],
            Vec::new(),
        );
        // Дефолт (оба бюджета 10000): ОБА гварда дают класс
        // E_LIMIT_EXCEEDED. Конд-вычисление стоит ~4 тика общего бюджета
        // (узлы AST) — глобальный MaxEvalSteps срабатывает раньше
        // per-loop счётчика; класс один и тот же (T5 детерминирован).
        let mut e = ev();
        let err = e.run(&program).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::LimitExceeded);
        assert!(matches!(
            err,
            L2ExecError::Eval(L2Error::MaxEvalSteps { limit: 10_000 })
                | L2ExecError::LoopLimitExceeded { limit: 10_000 }
        ));
        // Малый ОБЩИЙ бюджет: env-tick срабатывает раньше per-loop.
        let mut e2 = L2Evaluator::new(L2Config {
            max_eval_steps: 50,
            ..cfg()
        });
        assert!(matches!(
            e2.run(&program),
            Err(L2ExecError::Eval(L2Error::MaxEvalSteps { limit: 50 }))
        ));
        // Малый per-loop бюджет — T5 «на КАЖДЫЙ цикл»: 21-е вычисление
        // условия прерывает цикл раньше общего бюджета (21×4 тиков < 10000).
        let mut e3 = L2Evaluator::new(L2Config {
            max_loop_steps: 20,
            ..cfg()
        });
        assert_eq!(
            e3.run(&program).unwrap_err(),
            L2ExecError::LoopLimitExceeded { limit: 20 }
        );
    }

    #[test]
    fn t5_per_loop_budget_is_per_session() {
        // Два последовательных цикла по 5 итераций: per-loop лимит 6 —
        // ОБА исполняются (бюджет — на сессию, не на статический узел).
        let while5 = |body_idx: u32| L2Stmt::While {
            cond: Expr::Binary(
                BinaryOp::Lt,
                Box::new(Expr::var(body_idx)),
                Box::new(Expr::num(5.0)),
            ),
            do_id: 1,
            body: vec![assign(
                body_idx,
                Expr::Binary(
                    BinaryOp::Add,
                    Box::new(Expr::var(body_idx)),
                    Box::new(Expr::num(1.0)),
                ),
            )],
            src: placeholder_span(),
        };
        let program = L2Program::new(
            vec![
                assign(1, Expr::num(0.0)),
                while5(1),
                assign(2, Expr::num(0.0)),
                while5(2),
            ],
            Vec::new(),
        );
        let cfg = L2Config {
            max_loop_steps: 6,
            ..cfg()
        };
        let mut e = L2Evaluator::new(cfg);
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(1).unwrap(), Val::Num(5.0));
        assert_eq!(e.env().read_direct(2).unwrap(), Val::Num(5.0));
    }

    #[test]
    fn d2_while_do_id_domain_and_null_cond() {
        // Null-cond = false (нулевая итерация); do_id вне 1..3 — ошибка.
        let program = L2Program::new(
            vec![L2Stmt::While {
                cond: Expr::Null, // IF [#0] → false
                do_id: 1,
                body: vec![assign(500, Expr::num(1.0))],
                src: placeholder_span(),
            }],
            Vec::new(),
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(500).unwrap(), Val::Null);
        let bad = L2Program::new(
            vec![L2Stmt::While {
                cond: Expr::num(1.0),
                do_id: 5,
                body: vec![],
                src: placeholder_span(),
            }],
            Vec::new(),
        );
        assert_eq!(
            ev().run(&bad).unwrap_err().error_code(),
            ErrorCode::RuntimeState
        );
    }

    #[test]
    fn d2_bounded_while_executes() {
        // #1 = 3; WHILE [#1 GT 0] DO1 #1 = #1 - 1 END1 → #1 == 0.
        let program = L2Program::new(
            vec![
                assign(1, Expr::num(3.0)),
                L2Stmt::While {
                    cond: Expr::Binary(
                        BinaryOp::Gt,
                        Box::new(Expr::var(1)),
                        Box::new(Expr::num(0.0)),
                    ),
                    do_id: 1,
                    body: vec![assign(
                        1,
                        Expr::Binary(
                            BinaryOp::Sub,
                            Box::new(Expr::var(1)),
                            Box::new(Expr::num(1.0)),
                        ),
                    )],
                    src: placeholder_span(),
                },
            ],
            Vec::new(),
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(1).unwrap(), Val::Num(0.0));
    }

    // ---------- D3: мёртвые ветки/тривиальные условия ----------

    #[test]
    fn d3_dead_branch_not_evaluated_sys_stops_avoided() {
        // D3-синергия (§4): мёртвая ветка НЕ вычисляется — SysVar-чтение
        // (E_RUNTIME_STATE в живой ветке) в мёртвой не останавливает.
        let dead_move = move_stmt(
            MoveKind::Linear,
            ExprPose {
                x: Some(Expr::Var(VarRef::Sys(SysVar::SkipPos(Axis::X)))),
                y: Some(Expr::num(0.0)),
                z: Some(Expr::num(0.0)),
                a: None,
                b: None,
                c: None,
            },
            None,
        );
        // IF [1 EQ 2] THEN <Sys-ход> — ветка мертва: без останова.
        let program = L2Program::new(
            vec![L2Stmt::IfThen {
                cond: Expr::Binary(
                    BinaryOp::Eq,
                    Box::new(Expr::num(1.0)),
                    Box::new(Expr::num(2.0)),
                ),
                then_stmt: Box::new(dead_move.clone()),
                src: placeholder_span(),
            }],
            Vec::new(),
        );
        let mut e = ev();
        e.run(&program).unwrap();
        // IF [#500 GT 0] c #500 = Null (→0.0) — тоже мертва.
        let program = L2Program::new(
            vec![L2Stmt::IfThen {
                cond: Expr::Binary(
                    BinaryOp::Gt,
                    Box::new(Expr::var(500)),
                    Box::new(Expr::num(0.0)),
                ),
                then_stmt: Box::new(dead_move),
                src: placeholder_span(),
            }],
            Vec::new(),
        );
        let mut e = ev();
        e.run(&program).unwrap();
        // Контраст: ЖИВАЯ ветка с Sys-чтением → останов E_RUNTIME_STATE.
        let live = L2Program::new(
            vec![L2Stmt::IfThen {
                cond: Expr::Binary(
                    BinaryOp::Eq,
                    Box::new(Expr::num(1.0)),
                    Box::new(Expr::num(1.0)),
                ),
                then_stmt: Box::new(move_stmt(
                    MoveKind::Linear,
                    ExprPose {
                        x: Some(Expr::Var(VarRef::Sys(SysVar::SkipPos(Axis::X)))),
                        y: Some(Expr::num(0.0)),
                        z: Some(Expr::num(0.0)),
                        a: None,
                        b: None,
                        c: None,
                    },
                    None,
                )),
                src: placeholder_span(),
            }],
            Vec::new(),
        );
        assert_eq!(
            ev().run(&live).unwrap_err().error_code(),
            ErrorCode::RuntimeState
        );
    }

    #[test]
    fn d3_const_ifthen_selects_branch() {
        // IF [1 EQ 1] THEN (исполняется) / IF [0] / IF [#0] THEN (нет).
        let program = L2Program::new(
            vec![
                L2Stmt::IfThen {
                    cond: Expr::Binary(
                        BinaryOp::Eq,
                        Box::new(Expr::num(1.0)),
                        Box::new(Expr::num(1.0)),
                    ),
                    then_stmt: Box::new(assign(100, Expr::num(1.0))),
                    src: placeholder_span(),
                },
                L2Stmt::IfThen {
                    cond: Expr::num(0.0),
                    then_stmt: Box::new(assign(101, Expr::num(1.0))),
                    src: placeholder_span(),
                },
                L2Stmt::IfThen {
                    cond: Expr::Null, // IF [#0] — false
                    then_stmt: Box::new(assign(102, Expr::num(1.0))),
                    src: placeholder_span(),
                },
            ],
            Vec::new(),
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(100).unwrap(), Val::Num(1.0));
        assert_eq!(e.env().read_direct(101).unwrap(), Val::Null);
        assert_eq!(e.env().read_direct(102).unwrap(), Val::Null);
    }

    // ---------- SysVar-маппинг (§3) ----------

    #[test]
    fn sysvar_modal_group_snapshot_resolved() {
        // ModalGroup(1): power-on 0 (G0); после Linear → 1; после Arc Cw → 2.
        let mv = |v: Expr| linear(v, Expr::num(0.0), Expr::num(0.0), None);
        let program = L2Program::new(
            vec![
                move_stmt(MoveKind::Rapid, target_const(1.0, 0.0, 0.0), None),
                mv(Expr::num(2.0)),
                assign(500, Expr::Var(VarRef::Sys(SysVar::ModalGroup(1)))),
                mv(Expr::var(500)),
                assign(501, Expr::Var(VarRef::Sys(SysVar::ModalGroup(1)))),
                L2Stmt::Move {
                    kind: MoveKind::Arc {
                        ijk: [Some(Expr::num(-1.0)), Some(Expr::num(0.0)), None],
                        r: None,
                        dir: Dir::Cw,
                    },
                    target: target_const(1.0, 1.0, 0.0),
                    feed: None,
                    spindle: None,
                    src: placeholder_span(),
                },
                assign(502, Expr::Var(VarRef::Sys(SysVar::ModalGroup(1)))),
            ],
            Vec::new(),
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(500).unwrap(), Val::Num(1.0)); // G1
        assert_eq!(e.env().read_direct(501).unwrap(), Val::Num(1.0));
        assert_eq!(e.env().read_direct(502).unwrap(), Val::Num(2.0)); // G2
                                                                      // Плоскость: power-on G17.
        let m = Modals::new();
        assert_eq!(m.plane(), Plane::XY);
        assert_eq!(m.value(1), Some(0));
    }

    #[test]
    fn sysvar_unmodeled_group_and_wcs_and_positions() {
        // Немоделируемая группа → E_RUNTIME_STATE; WcsOffset →
        // E_REQUIRES_MACHINE_TABLE; позиции → E_RUNTIME_STATE (§3).
        let read =
            |sv: SysVar| L2Program::new(vec![assign(500, Expr::Var(VarRef::Sys(sv)))], Vec::new());
        let err = ev().run(&read(SysVar::ModalGroup(77))).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::RuntimeState);
        let err = ev()
            .run(&read(SysVar::WcsOffset {
                p: 1,
                axis: Axis::X,
            }))
            .unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::RequiresMachineTable);
        let err = ev().run(&read(SysVar::WorkPos(Axis::Z))).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::RuntimeState);
        let err = ev().run(&read(SysVar::VendorSpecific(5223))).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::RuntimeState);
    }

    #[test]
    fn g31_sysvar_read_in_move_stops_with_runtime_state() {
        // §3 последняя строка: ход с X = #SKIP[X] — останов, программа
        // остаётся в L2 (Err; результата lowering нет).
        let mk = |v: Expr| {
            L2Program::new(
                vec![linear(
                    v,
                    Expr::num(0.0),
                    Expr::num(0.0),
                    Some(Expr::num(10.0)),
                )],
                Vec::new(),
            )
        };
        let skip = mk(Expr::Var(VarRef::Sys(SysVar::SkipPos(Axis::X))));
        let err = ev().run(&skip).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::RuntimeState);
        let abs = mk(Expr::Var(VarRef::Sys(SysVar::AbsMachinePos(Axis::Z))));
        assert_eq!(
            ev().run(&abs).unwrap_err().error_code(),
            ErrorCode::RuntimeState
        );
    }

    // ---------- Трансформации (§3 строки 3-4; синтетика — ценз B) ----------

    #[test]
    fn transform_rotate90_golden_end_to_end() {
        // G68 R90 вокруг (10,10): ход (15,10) → базовая (10,15) — бриф-пин.
        let program = L2Program::new(
            vec![L2Stmt::WithTransform {
                transform: TransformKind::Rotate2D {
                    center: [Expr::num(10.0), Expr::num(10.0)],
                    angle_deg: Expr::num(90.0),
                    plane: Plane::XY,
                },
                body: vec![move_stmt(
                    MoveKind::Rapid,
                    target_const(15.0, 10.0, 0.0),
                    None,
                )],
                src: placeholder_span(),
            }],
            Vec::new(),
        );
        let res = ev().run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 1);
        assert!(res.raw_tail.is_empty());
        if let Op::Rapid { end, .. } = &res.l1_ops[0] {
            assert_eq!((end.x.value(), end.y.value()), (10.0, 15.0));
        }
    }

    #[test]
    fn transform_rotate_applies_to_multiple_moves_and_unwinds() {
        // 45° вокруг origin: (10,0) → (7.0711, 7.0711); после блока
        // трансформация снята — следующий ход без поворота.
        let program = L2Program::new(
            vec![
                L2Stmt::WithTransform {
                    transform: TransformKind::Rotate2D {
                        center: [Expr::num(0.0), Expr::num(0.0)],
                        angle_deg: Expr::num(45.0),
                        plane: Plane::XY,
                    },
                    body: vec![move_stmt(
                        MoveKind::Rapid,
                        target_const(10.0, 0.0, 0.0),
                        None,
                    )],
                    src: placeholder_span(),
                },
                move_stmt(MoveKind::Rapid, target_const(10.0, 0.0, 0.0), None),
            ],
            Vec::new(),
        );
        let res = ev().run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 2);
        let get_xy = |op: &Op| match op {
            Op::Rapid { end, .. } => (end.x.value(), end.y.value()),
            _ => panic!(),
        };
        let (x1, y1) = get_xy(&res.l1_ops[0]);
        // Значения Num — 4dp (single-final-rounding, SPEC §10.5):
        // 7.07106781 → 7.0711.
        assert_eq!(x1, 7.0711);
        assert_eq!(y1, 7.0711);
        assert_eq!(get_xy(&res.l1_ops[1]), (10.0, 0.0)); // трансформация снята
    }

    #[test]
    fn transform_scale_golden_and_arc_offsets() {
        // G51 P2 вокруг (0,0,0): (3,4,5) → (6,8,10); дуга ПЕРВОЙ в теле:
        // программный старт (10,0,0), I-5 J0 → программный центр (5,0),
        // конец (0,5); масштаб ×2 → базовые старт (20,0), центр (10,0),
        // конец (0,10) (смещения IJK — векторы, масштабируются тоже);
        // углы масштаб не меняет: старт 0°, конец 135° → Cw sweep 225°.
        let arc = L2Stmt::Move {
            kind: MoveKind::Arc {
                ijk: [Some(Expr::num(-5.0)), Some(Expr::num(0.0)), None],
                r: None,
                dir: Dir::Cw,
            },
            target: target_const(0.0, 5.0, 0.0),
            feed: None,
            spindle: None,
            src: placeholder_span(),
        };
        let program = L2Program::new(
            vec![
                move_stmt(MoveKind::Rapid, target_const(10.0, 0.0, 0.0), None),
                L2Stmt::WithTransform {
                    transform: TransformKind::Scale {
                        center: [Expr::num(0.0), Expr::num(0.0), Expr::num(0.0)],
                        factor: Expr::num(2.0),
                    },
                    body: vec![
                        arc,
                        move_stmt(MoveKind::Rapid, target_const(3.0, 4.0, 5.0), None),
                    ],
                    src: placeholder_span(),
                },
            ],
            Vec::new(),
        );
        let res = ev().run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 3);
        if let Op::Arc {
            start,
            end,
            center,
            sweep_deg,
            ..
        } = &res.l1_ops[1]
        {
            match start {
                StartPoint::Known(p) => {
                    assert_eq!((p.x.value(), p.y.value()), (20.0, 0.0));
                }
                _ => panic!("arc start must be known"),
            }
            assert_eq!((end.x.value(), end.y.value()), (0.0, 10.0));
            assert_eq!((center.x.value(), center.y.value()), (10.0, 0.0));
            assert!((sweep_deg.value() - 225.0).abs() < 1e-6);
        } else {
            panic!("expected Arc");
        }
        if let Op::Rapid { end, .. } = &res.l1_ops[2] {
            assert_eq!(
                (end.x.value(), end.y.value(), end.z.value()),
                (6.0, 8.0, 10.0)
            );
        }
    }

    #[test]
    fn transform_nested_compose_inner_first() {
        // Scale(2, origin) внутри Rotate(90°, origin): точка (1,0,0):
        // сначала scale → (2,0), затем rotate → (0,2) (семантика вложенных
        // кадров: активная = внешняя ∘ новая).
        let program = L2Program::new(
            vec![L2Stmt::WithTransform {
                transform: TransformKind::Rotate2D {
                    center: [Expr::num(0.0), Expr::num(0.0)],
                    angle_deg: Expr::num(90.0),
                    plane: Plane::XY,
                },
                body: vec![L2Stmt::WithTransform {
                    transform: TransformKind::Scale {
                        center: [Expr::num(0.0), Expr::num(0.0), Expr::num(0.0)],
                        factor: Expr::num(2.0),
                    },
                    body: vec![move_stmt(
                        MoveKind::Rapid,
                        target_const(1.0, 0.0, 0.0),
                        None,
                    )],
                    src: placeholder_span(),
                }],
                src: placeholder_span(),
            }],
            Vec::new(),
        );
        let res = ev().run(&program).unwrap();
        if let Op::Rapid { end, .. } = &res.l1_ops[0] {
            assert!((end.x.value() - 0.0).abs() < 1e-9);
            assert!((end.y.value() - 2.0).abs() < 1e-9);
        }
    }

    #[test]
    fn transform_arc_center_rotation() {
        // Поворот 90° вокруг origin: дуга из (10,0) c I-5 J0 (центр (5,0)):
        // старт (0,10), смещение-вектор (−5,0) → (0,−5), центр (0,5);
        // конец (5,5) → (−5,5); sweep сохраняется 90° CCW.
        let program = L2Program::new(
            vec![
                move_stmt(MoveKind::Rapid, target_const(10.0, 0.0, 0.0), None),
                L2Stmt::WithTransform {
                    transform: TransformKind::Rotate2D {
                        center: [Expr::num(0.0), Expr::num(0.0)],
                        angle_deg: Expr::num(90.0),
                        plane: Plane::XY,
                    },
                    body: vec![L2Stmt::Move {
                        kind: MoveKind::Arc {
                            ijk: [Some(Expr::num(-5.0)), Some(Expr::num(0.0)), None],
                            r: None,
                            dir: Dir::Ccw,
                        },
                        target: target_const(5.0, 5.0, 0.0),
                        feed: None,
                        spindle: None,
                        src: placeholder_span(),
                    }],
                    src: placeholder_span(),
                },
            ],
            Vec::new(),
        );
        let res = ev().run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 2);
        if let Op::Arc {
            end,
            center,
            sweep_deg,
            normal,
            ..
        } = &res.l1_ops[1]
        {
            assert!((end.x.value() + 5.0).abs() < 1e-6);
            assert!((end.y.value() - 5.0).abs() < 1e-6);
            assert!((center.x.value() - 0.0).abs() < 1e-6);
            assert!((center.y.value() - 5.0).abs() < 1e-6);
            assert!((sweep_deg.value() - 90.0).abs() < 1e-6);
            assert_eq!(
                (normal.x.value(), normal.y.value(), normal.z.value()),
                (0.0, 0.0, 1.0)
            );
        }
    }

    #[test]
    fn t6_mirror_default_refusal() {
        // G51.1 без флага → отказ E_UNSUPPORTED_SEMANTICS (§3 строка 4).
        let program = L2Program::new(
            vec![L2Stmt::WithTransform {
                transform: TransformKind::Mirror {
                    axis: Axis::X,
                    pivot: Expr::num(0.0),
                },
                body: vec![move_stmt(
                    MoveKind::Rapid,
                    target_const(5.0, 0.0, 0.0),
                    None,
                )],
                src: placeholder_span(),
            }],
            Vec::new(),
        );
        let err = ev().run(&program).unwrap_err();
        assert_eq!(err.error_code(), ErrorCode::UnsupportedSemantics);
        assert!(err.message().contains("allow_mirror_bake"));
    }

    #[test]
    fn t6_mirror_baked_inverts_and_flips_arc_dir_with_notice() {
        // allow_mirror_bake = true: X инвертируется (x ↦ −x, pivot 0),
        // дуга Cw → Ccw, Notice-диагностика выставлена.
        let arc = L2Stmt::Move {
            kind: MoveKind::Arc {
                ijk: [Some(Expr::num(-5.0)), Some(Expr::num(0.0)), None],
                r: None,
                dir: Dir::Cw,
            },
            target: target_const(0.0, 5.0, 0.0),
            feed: None,
            spindle: None,
            src: placeholder_span(),
        };
        let program = L2Program::new(
            vec![
                move_stmt(MoveKind::Rapid, target_const(10.0, 0.0, 0.0), None),
                L2Stmt::WithTransform {
                    transform: TransformKind::Mirror {
                        axis: Axis::X,
                        pivot: Expr::num(0.0),
                    },
                    body: vec![
                        arc,
                        move_stmt(MoveKind::Rapid, target_const(5.0, 0.0, 0.0), None),
                    ],
                    src: placeholder_span(),
                },
            ],
            Vec::new(),
        );
        let cfg = L2Config {
            allow_mirror_bake: true,
            ..cfg()
        };
        let mut e = L2Evaluator::new(cfg);
        let res = e.run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 3);
        // Notice.
        assert!(res
            .diagnostics
            .iter()
            .any(|d| d.message.contains("mirror baked")));
        // Дуга ПЕРВОЙ в теле: зеркальный старт (−10,0), I зеркалится
        // (−5 → 5) → центр (−5,0); конец (0,5) → (0,5); dir Cw→Ccw;
        // углы: старт 180°, конец 45° → CCW sweep 225° (величина свипа
        // сохраняется — зеркало меняет ориентацию, флип dir компенсирует).
        if let Op::Arc {
            start,
            end,
            dir,
            center,
            sweep_deg,
            ..
        } = &res.l1_ops[1]
        {
            match start {
                StartPoint::Known(p) => assert_eq!((p.x.value(), p.y.value()), (-10.0, 0.0)),
                _ => panic!("arc start must be known"),
            }
            assert_eq!((end.x.value(), end.y.value()), (0.0, 5.0));
            assert_eq!(*dir, Dir::Ccw);
            assert_eq!((center.x.value(), center.y.value()), (-5.0, 0.0));
            assert!((sweep_deg.value() - 225.0).abs() < 1e-6);
        }
        // Rapid после дуги: (5,0,0) → (−5,0,0).
        if let Op::Rapid { end, .. } = &res.l1_ops[2] {
            assert_eq!((end.x.value(), end.y.value()), (-5.0, 0.0));
        }
    }

    #[test]
    fn transform_twp_is_carrier_with_suppressed_body() {
        // G68.2 → Raw-носитель с подставленными значениями; тело подавлено
        // (§3 строка 6; синтетический порядок слов X Y Z A B C — ценз B).
        let program = L2Program::new(
            vec![L2Stmt::WithTransform {
                transform: TransformKind::TiltedWorkPlane {
                    origin: [Expr::num(1.0), Expr::num(2.0), Expr::num(3.0)],
                    euler: [Expr::num(10.0), Expr::num(20.0), Expr::num(30.0)],
                },
                body: vec![move_stmt(
                    MoveKind::Rapid,
                    target_const(5.0, 0.0, 0.0),
                    None,
                )],
                src: placeholder_span(),
            }],
            Vec::new(),
        );
        let res = ev().run(&program).unwrap();
        assert!(res.l1_ops.is_empty(), "body must be suppressed");
        assert_eq!(res.raw_tail.len(), 1);
        assert_eq!(res.raw_tail[0].marker, RawMarker::Transform);
        assert_eq!(
            decode_payload(&res.raw_tail[0].op),
            "G68.2 X1.0000 Y2.0000 Z3.0000 A10.0000 B20.0000 C30.0000\n"
        );
    }

    #[test]
    fn rtcp_is_carrier_payload_with_h() {
        // G43.4 H1 / G49 → Raw-носители (§3 строка 6).
        let program = L2Program::new(
            vec![
                L2Stmt::RtcpMode {
                    enabled: true,
                    tool_length_h: Some(1),
                    src: placeholder_span(),
                },
                L2Stmt::RtcpMode {
                    enabled: false,
                    tool_length_h: None,
                    src: placeholder_span(),
                },
            ],
            Vec::new(),
        );
        let res = ev().run(&program).unwrap();
        assert_eq!(res.raw_tail.len(), 2);
        assert_eq!(res.raw_tail[0].marker, RawMarker::Rtcp);
        assert_eq!(decode_payload(&res.raw_tail[0].op), "G43.4 H1\n");
        assert_eq!(decode_payload(&res.raw_tail[1].op), "G49\n");
    }

    // ---------- Дуги: lowering в Op::Arc ----------

    #[test]
    fn arc_ijk_lowers_to_l1_with_canonical_geometry() {
        // G3 из (10,0) c I-5 J0 в (5,5): Op::Arc center (5,0), sweep 90,
        // нормаль +Z, pitch 0; start Known (связность).
        let program = L2Program::new(
            vec![
                move_stmt(MoveKind::Rapid, target_const(10.0, 0.0, 0.0), None),
                L2Stmt::Move {
                    kind: MoveKind::Arc {
                        ijk: [Some(Expr::num(-5.0)), Some(Expr::num(0.0)), None],
                        r: None,
                        dir: Dir::Ccw,
                    },
                    target: target_const(5.0, 5.0, 0.0),
                    feed: Some(Expr::num(100.0)),
                    spindle: None,
                    src: placeholder_span(),
                },
            ],
            Vec::new(),
        );
        let res = ev().run(&program).unwrap();
        assert_eq!(res.l1_ops.len(), 2);
        assert!(res.raw_tail.is_empty());
        match &res.l1_ops[1] {
            Op::Arc {
                start,
                end,
                center,
                normal,
                dir,
                sweep_deg,
                pitch,
                state,
                ..
            } => {
                match start {
                    StartPoint::Known(p) => assert_eq!((p.x.value(), p.y.value()), (10.0, 0.0)),
                    StartPoint::Unknown => panic!("start must be known"),
                }
                assert_eq!((end.x.value(), end.y.value()), (5.0, 5.0));
                assert_eq!((center.x.value(), center.y.value()), (5.0, 0.0));
                assert_eq!(
                    (normal.x.value(), normal.y.value(), normal.z.value()),
                    (0.0, 0.0, 1.0)
                );
                assert_eq!(*dir, Dir::Ccw);
                assert!((sweep_deg.value() - 90.0).abs() < 1e-9);
                assert!(pitch.value().abs() < 1e-12);
                assert_eq!(state.feed.as_ref().map(|f| f.value()), Some(100.0));
            }
            _ => panic!("expected Arc"),
        }
    }

    #[test]
    fn arc_r_form_and_helical_pitch_and_invalid() {
        // R-форма: (0,0) → (10,0) R5 CCW: центр (5,0), sweep 180.
        // Винт: K-компонента хода → pitch; K в центре ≠ 0 → носитель.
        // Без IJK/R → носитель InvalidInput.
        let mk = |kind: MoveKind, tgt: ExprPose, feed: Option<Expr>| {
            L2Program::new(
                vec![
                    move_stmt(MoveKind::Rapid, target_const(0.0, 0.0, 0.0), None),
                    L2Stmt::Move {
                        kind,
                        target: tgt,
                        feed,
                        spindle: None,
                        src: placeholder_span(),
                    },
                ],
                Vec::new(),
            )
        };
        let r_prog = mk(
            MoveKind::Arc {
                ijk: [None, None, None],
                r: Some(Expr::num(5.0)),
                dir: Dir::Ccw,
            },
            target_const(10.0, 0.0, 0.0),
            None,
        );
        let res = ev().run(&r_prog).unwrap();
        match &res.l1_ops[1] {
            Op::Arc {
                center, sweep_deg, ..
            } => {
                assert!((center.x.value() - 5.0).abs() < 1e-6);
                assert!((sweep_deg.value() - 180.0).abs() < 1e-6);
            }
            _ => panic!("expected Arc"),
        }
        // Винт: конец (5,5,3), IJK без K → pitch 3.
        let helix = mk(
            MoveKind::Arc {
                ijk: [Some(Expr::num(-5.0)), Some(Expr::num(0.0)), None],
                r: None,
                dir: Dir::Ccw,
            },
            target_const(5.0, 5.0, 3.0),
            None,
        );
        let res = ev().run(&helix).unwrap();
        match &res.l1_ops[1] {
            Op::Arc { pitch, .. } => assert!((pitch.value() - 3.0).abs() < 1e-9),
            _ => panic!("expected Arc"),
        }
        // K-смещение центра ≠ 0 → носитель InvalidInput.
        let bad = mk(
            MoveKind::Arc {
                ijk: [
                    Some(Expr::num(-5.0)),
                    Some(Expr::num(0.0)),
                    Some(Expr::num(2.0)),
                ],
                r: None,
                dir: Dir::Ccw,
            },
            target_const(5.0, 5.0, 3.0),
            None,
        );
        let res = ev().run(&bad).unwrap();
        assert_eq!(res.raw_tail.len(), 1);
        assert_eq!(res.raw_tail[0].marker, RawMarker::ArcUnlowered);
        match &res.raw_tail[0].op {
            Op::Raw { code, .. } => assert_eq!(*code, ErrorCode::InvalidInput),
            _ => panic!(),
        }
        // Ни IJK, ни R → носитель InvalidInput (NoGeometry).
        let none = mk(
            MoveKind::Arc {
                ijk: [None, None, None],
                r: None,
                dir: Dir::Ccw,
            },
            target_const(5.0, 5.0, 0.0),
            None,
        );
        let res = ev().run(&none).unwrap();
        assert_eq!(res.raw_tail.len(), 1);
        assert!(res.diagnostics[0]
            .message
            .contains("without in-plane IJK/R"));
        // Старт неизвестен → носитель RuntimeState.
        let nostart = L2Program::new(
            vec![L2Stmt::Move {
                kind: MoveKind::Arc {
                    ijk: [Some(Expr::num(-5.0)), None, None],
                    r: None,
                    dir: Dir::Ccw,
                },
                target: target_const(5.0, 5.0, 0.0),
                feed: None,
                spindle: None,
                src: placeholder_span(),
            }],
            Vec::new(),
        );
        let res = ev().run(&nostart).unwrap();
        assert_eq!(res.raw_tail[0].marker, RawMarker::ArcUnlowered);
        assert_eq!(res.diagnostics[0].code, ErrorCode::RuntimeState);
    }

    // ---------- Инфраструктура ----------

    #[test]
    fn raw_payload_hex_is_deterministic() {
        let text = "G0 X15.0000 A15.0000\n";
        assert_eq!(
            crate::lowering::payload_hex(text),
            crate::lowering::payload_hex(text)
        );
        let op = crate::lowering::raw_frame_op(
            text,
            "r".into(),
            ErrorCode::UnsupportedSemantics,
            placeholder_span(),
        );
        if let Op::Raw { payload_hex, .. } = &op {
            assert!(!payload_hex.is_empty());
            assert!(payload_hex.chars().all(|c| c.is_ascii_hexdigit()));
            assert!(payload_hex.ends_with("0A"), "terminator \\n in hex tail");
        } else {
            panic!("carrier must be Op::Raw");
        }
    }

    #[test]
    fn l2_stmt_src_accessor() {
        let mv = move_stmt(MoveKind::Rapid, ExprPose::empty(), None);
        assert!(mv.src().is_some());
        assert!(label(1).src().is_some());
        assert!(L2Stmt::Return {
            src: placeholder_span()
        }
        .src()
        .is_some());
        assert!(L2Stmt::L1Op(Op::ProgramEnd {
            src: placeholder_span()
        })
        .src()
        .is_none());
    }

    #[test]
    fn call_arg_local_index_mapping() {
        // Амендмент v0.1.2 (BUG-D1): Fanuc Macro B G65-таблица (Haas
        // идентична), регистронезависимо; не-аргумент/не-буква → None.
        let fanuc: [(u8, u32); 21] = [
            (b'A', 1),
            (b'B', 2),
            (b'C', 3),
            (b'I', 4),
            (b'J', 5),
            (b'K', 6),
            (b'D', 7),
            (b'E', 8),
            (b'F', 9),
            (b'H', 11),
            (b'M', 13),
            (b'Q', 17),
            (b'R', 18),
            (b'S', 19),
            (b'T', 20),
            (b'U', 21),
            (b'V', 22),
            (b'W', 23),
            (b'X', 24),
            (b'Y', 25),
            (b'Z', 26),
        ];
        for (l, slot) in fanuc {
            assert_eq!(crate::ast::call_arg_local_index(l), Some(slot), "{l}");
        }
        // G/L/N/O/P — НЕ аргументы G65; небуква → None.
        for l in [b'G', b'L', b'N', b'O', b'P', b'1', b'#'] {
            assert_eq!(crate::ast::call_arg_local_index(l), None, "{l}");
        }
        // Регистронезависимость (lowercase D → Fanuc #7).
        assert_eq!(crate::ast::call_arg_local_index(b'd'), Some(7));
        // Позиционная таблица (LinuxCNC o-call) — линейна на всём A..Z
        // (включая G/L/N/O/P), не-буква → None.
        assert_eq!(crate::ast::positional_arg_local_index(b'D'), Some(4));
        assert_eq!(crate::ast::positional_arg_local_index(b'G'), Some(7));
        assert_eq!(crate::ast::positional_arg_local_index(b'P'), Some(16));
        assert_eq!(crate::ast::positional_arg_local_index(b'Z'), Some(26));
        assert_eq!(crate::ast::positional_arg_local_index(b'1'), None);
    }

    #[test]
    fn d1_fanuc_table_binding_deep_drill_letters() {
        // BUG-D1 пин (trace-зеркало deep-drill): драйвер
        // `G65 P7981 Z-7.88 D-.4 S6000 F60.` — буквы НЕ образуют A..-префикс
        // → Fanuc-таблица: D→#7, F→#9, Z→#26, S→#19 (линейная привязка
        // давала #4/#6 → guard IF[#7 EQ #0] TRUE → alarm-путь).
        let program = L2Program::new(
            vec![call(
                SubTarget::ONumber(7981),
                vec![
                    (b'Z', Expr::num(-7.88)),
                    (b'D', Expr::num(-0.4)),
                    (b'S', Expr::num(6000.0)),
                    (b'F', Expr::num(60.0)),
                ],
            )],
            vec![crate::ast::L2Sub {
                target: SubTarget::ONumber(7981),
                body: vec![
                    assign(500, Expr::var(7)),
                    assign(501, Expr::var(9)),
                    assign(502, Expr::var(26)),
                    assign(503, Expr::var(19)),
                ],
            }],
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(500).unwrap(), Val::Num(-0.4));
        assert_eq!(e.env().read_direct(501).unwrap(), Val::Num(60.0));
        assert_eq!(e.env().read_direct(502).unwrap(), Val::Num(-7.88));
        assert_eq!(e.env().read_direct(503).unwrap(), Val::Num(6000.0));
    }

    #[test]
    fn d1_positional_binding_linuxcnc_prefix_letters() {
        // Различитель таблиц (см. call_arg_index): точный A..-префикс →
        // ПОЗИЦИОННАЯ привязка (LinuxCNC o call: 4-й arg D → #4, НЕ
        // Fanuc-#7; #7 остаётся несвязанной = Null).
        let program = L2Program::new(
            vec![call(
                SubTarget::ONumber(1),
                vec![
                    (b'A', Expr::num(10.0)),
                    (b'B', Expr::num(20.0)),
                    (b'C', Expr::num(30.0)),
                    (b'D', Expr::num(40.0)),
                ],
            )],
            vec![crate::ast::L2Sub {
                target: SubTarget::ONumber(1),
                body: vec![assign(500, Expr::var(4)), assign(501, Expr::var(7))],
            }],
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(500).unwrap(), Val::Num(40.0), "D→#4");
        assert_eq!(e.env().read_direct(501).unwrap(), Val::Null, "#7 unbound");
    }

    #[test]
    fn run_is_repeatable_and_resets_state() {
        // Прогон детерминирован: состояние (переменные/позиция/модальности/
        // трансформации) сбрасывается на каждый run.
        let program = L2Program::new(vec![assign(100, Expr::num(5.0))], Vec::new());
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(100).unwrap(), Val::Num(5.0));
        // Второй прогон той же программы — тот же результат.
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(100).unwrap(), Val::Num(5.0));
    }

    // ---------- SubTarget::Named (амендрт v0.1.2, парсер D2) ----------

    #[test]
    fn d1_named_subtarget_resolution() {
        // o<square> sub / call — именованная подпрограмма.
        let program = L2Program::new(
            vec![call(
                SubTarget::Named("square".to_string()),
                vec![(b'A', Expr::num(3.0))],
            )],
            vec![crate::ast::L2Sub {
                target: SubTarget::Named("square".to_string()),
                body: vec![assign(
                    100,
                    Expr::Binary(
                        BinaryOp::Mul,
                        Box::new(Expr::var(1)),
                        Box::new(Expr::var(1)),
                    ),
                )],
            }],
        );
        let mut e = ev();
        e.run(&program).unwrap();
        assert_eq!(e.env().read_direct(100).unwrap(), Val::Num(9.0));
    }
}
