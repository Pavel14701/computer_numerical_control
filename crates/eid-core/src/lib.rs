//! eid-core — AST, диагностика, EID-парсер/эмиттер (Агент A, IMPLEMENTATION-PLAN §2-A).
//! Интерфейсы заморожены: v0.1-interface (итерация 0). Изменение сигнатур —
//! только PR к оркестратору.
#![allow(dead_code)]

pub mod adapter;
pub mod ast;
pub mod diag;
pub mod emitter;
pub mod parser;
