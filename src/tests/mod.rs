use core::panic::PanicInfo;

use crate::{init, serial_print};

#[macro_use]
mod serial;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) -> ! {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut port = Port::new(0xf4);
        port.write(exit_code as u32);
    }

    println!("Exit did not succeed, looping");

    loop {
        x86_64::instructions::hlt();
    }
}

/// This function is called on panic in a test build.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    serial_println!("[failure]");
    serial_println!("{}", info);

    exit_qemu(QemuExitCode::Failed);
}

pub trait Testable {
    fn run(&self);
}

impl<T> Testable for T
where
    T: Fn(),
{
    fn run(&self) {
        serial_print!("{}...\t", core::any::type_name::<T>());
        self();
        serial_println!("[ok]");
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    init();

    // Calls the test harness which was re-exported in crate root
    crate::test_main();

    exit_qemu(QemuExitCode::Success);
}

pub fn test_runner(tests: &[&dyn Testable]) {
    println!("Running {} tests", tests.len());

    for test in tests {
        test.run();
    }

    println!("All tests passed");
}

#[test_case]
fn always_passes() {
    println!("Always passing test");
}
