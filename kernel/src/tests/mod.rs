use core::panic::PanicInfo;

use bootloader_api::BootInfo;

use crate::{cpu, init, println, serial, serial_println, BOOT_CONFIG};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum QemuExitCode {
    Success = 0x10,
    Failed = 0x11,
}

pub fn exit_qemu(exit_code: QemuExitCode) -> ! {
    use x86_64::instructions::port::Port;

    // SAFETY:
    // This port should exit the program immediately if running under QEMU.
    // This code should only be compiled when running tests, so it only needs to work under QEMU anyway.
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
    println!("{}", info);

    let stack_pointer_approx = info as *const _ as usize;

    println!(
        "Current stack pointer is approximately {:#x}",
        stack_pointer_approx
    );
    println!("In stack {:?}", cpu::gdt::get_stack(stack_pointer_approx));

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
        println!("{}", core::any::type_name::<T>());
        self();
    }
}

bootloader_api::entry_point!(kernel_main, config = &BOOT_CONFIG);

/// The kernel's entry point when running tests
fn kernel_main(boot_info: &'static mut BootInfo) -> ! {
    // SAFETY:
    // This is the entry point for the program, so init() cannot have been run before.
    // This code runs with kernel privileges
    unsafe {
        init(boot_info);
    }

    // Calls the test harness which was re-exported in crate root
    crate::test_main();

    exit_qemu(QemuExitCode::Success);
}

/// The runner for a test. Because of the way the host-side of the test runner is written,
/// this function responds to two different types of command, read from serial input:
///
/// * `count`: Writes the number of tests to serial output.
/// * `run`: Reads the test number from serial input and runs it.
pub fn test_runner(tests: &[&dyn Testable]) {
    // This is so that the host test runner script knows when to send the command
    println!(">>>>>> READY FOR TEST COMMAND");
    let command = serial::readln();

    match command.as_str() {
        "count" => {
            serial_println!("{}", tests.len());
        }
        "run" => {
            let i = serial::readln().parse::<usize>().unwrap();
            let test = tests[i];
            test.run();
        }
        _ => panic!("Unknown command {command:?}"),
    }
}

#[test_case]
fn always_passes() {
    println!("Always passing test");
}
