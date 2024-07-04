//! Code for logging data using the [`log`] crate,

use log::Log;

use crate::graphics::{Colour, WRITER};
use crate::{print, println};

/// The kernel's implementation of the [`Log`] trait for printing logs
struct KernelLogger;

impl Log for KernelLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        let target = metadata.target();
        match metadata.level() {
            log::Level::Error => true,
            log::Level::Warn => true,
            log::Level::Trace | log::Level::Debug | log::Level::Info => {
                #[allow(clippy::needless_bool)] // For easier modification of this code in future
                if target.starts_with("acpi") || target.starts_with("ps2") {
                    false
                } else {
                    true
                }
            }
        }
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        print!("[");

        let colour = match record.level() {
            log::Level::Error => Colour::RED,
            log::Level::Warn => Colour::YELLOW,
            log::Level::Info => Colour::WHITE,
            log::Level::Debug => Colour::BLUE,
            log::Level::Trace => Colour::GREEN,
        };

        if let Ok(mut w) = WRITER.try_locked_if_init() {
            w.set_colour(colour);
        }

        let level_str = match record.level() {
            log::Level::Error => "ERROR",
            log::Level::Warn => "WARNING",
            log::Level::Info => "INFO",
            log::Level::Debug => "DEBUG",
            log::Level::Trace => "TRACE",
        };

        print!("{level_str}");

        if let Ok(mut w) = WRITER.try_locked_if_init() {
            w.set_colour(Colour::WHITE);
        }

        match (record.module_path(), record.file()) {
            // If the record is an error, print the whole file path not just the module
            (_, Some(file)) if record.level() == log::Level::Error => {
                print!(" {file}");
                if let Some(line) = record.line() {
                    print!(":{line}");
                }
            }
            (Some(module), _) => {
                print!(" {module}");
                if let Some(line) = record.line() {
                    print!(":{line}");
                }
            }
            _ => (),
        }

        print!("] ");

        println!("{}", record.args());
    }

    fn flush(&self) {}
}

/// Sets up logging for the kernel
pub fn init_log() {
    log::set_logger(&KernelLogger).expect("Logging should have initialised");
    log::set_max_level(log::LevelFilter::Trace);
}
