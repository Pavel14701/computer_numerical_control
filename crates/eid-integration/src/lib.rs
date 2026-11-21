//! eid-integration — profile, CLI-логика, симуляция, cross-dialect R3
//! (Агент E, IMPLEMENTATION-PLAN §2-E).
//!
//! Итерация 4: sim.rs (mock-sim, критерий #8) + translate.rs (live-конвейер
//! CLI через registry адаптеров — критерии #9/#10). Итерация 6 (аудит #24):
//! `#![allow(dead_code)]` снят — мёртвый код не сохраняется.

pub mod profile;
pub mod sim;
pub mod translate;
