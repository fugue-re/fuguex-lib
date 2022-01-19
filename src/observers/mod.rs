// pub mod observe_interrupt_handler;
// pub mod stubs;
// pub mod dummy_peripheral;
pub mod trace;
// pub use dummy_peripheral::*;
// pub use stubs::*;
pub use trace::*;
// mod solver;

pub mod watchpoint;
pub mod dummy_peripheral;
mod solver;