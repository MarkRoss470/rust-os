mod gdt;
mod idt;

pub use gdt::init_gdt;
pub use idt::init_idt;
pub use idt::init_pic;
