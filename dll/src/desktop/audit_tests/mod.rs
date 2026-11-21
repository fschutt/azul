//! Guards of the 2026-11-21 performance and correctness audit, one module per area.
//!
//! Each test names the finding it guards (`audit/<area>.md` at the repository root).
//! A test that needs a private item lives next to that item instead of here.

mod a1;
mod a2_cpu;
mod a2_gpu;
mod a3_core;
mod a3_solver;
mod a3_window;
mod a4;
mod a5;
mod a6;
mod a8;
mod a9;
mod a10;
mod a11;
