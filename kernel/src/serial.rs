//! [`serial_print!`][crate::serial_print!] and [`serial_println!`][crate::serial_println!] macros for writing to serial port

use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;
use x86_64::instructions::interrupts;

lazy_static! {
    pub static ref SERIAL1: Mutex<SerialPort> = {
        // SAFETY:
        // This just assumes a serial port exists on this port, which may not be the case
        // TODO: detect whether there really is a serial port
        let mut serial_port = unsafe { SerialPort::new(0x3F8) };
        serial_port.init();
        Mutex::new(serial_port)
    };
}

#[doc(hidden)]
pub fn _print(args: core::fmt::Arguments) {
    use core::fmt::Write;

    // Disable interrupts while locking mutex to prevent deadlocks
    interrupts::without_interrupts(|| {
        SERIAL1
            .lock()
            .write_fmt(args)
            .expect("Printing to serial failed");
    });
}

/// Prints to the host through the serial interface.
#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::_print(format_args!($($arg)*));
    };
}

/// Prints to the host through the serial interface, appending a newline.
#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($fmt:expr) => ($crate::serial_print!(concat!($fmt, "\n")));
    ($fmt:expr, $($arg:tt)*) => ($crate::serial_print!(
        concat!($fmt, "\n"), $($arg)*));
}

/// Reads a byte from the serial input.
///
/// This function will block if no data is sent to the serial port, so should only be called if this is guaranteed.
/// This function is intended to be used to read commands from the test handler (see [`test_runner`])
///
/// [`test_runner`]: crate::tests::test_runner
#[cfg(test)]
pub fn read() -> u8 {
    // Disable interrupts while locking mutex to prevent deadlocks
    interrupts::without_interrupts(|| SERIAL1.lock().receive())
}

#[cfg(test)]
use alloc::{
    string::{String, ToString},
    vec::Vec,
};

/// Reads a line from the serial input.
///
/// This function will block if no data is sent to the serial port, so should only be called if this is guaranteed.
/// This function is intended to be used to read commands from the test handler (see [`test_runner`])
///
/// [`test_runner`]: crate::tests::test_runner
#[cfg(test)]
pub fn readln() -> String {
    let mut s = Vec::new();

    loop {
        // Disable interrupts while locking mutex to prevent deadlocks
        let b = read();

        if b == b'\n' {
            break;
        } else {
            s.push(b);
        }
    }

    String::from_utf8_lossy(&s).to_string()
}
