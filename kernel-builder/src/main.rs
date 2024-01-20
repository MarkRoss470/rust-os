use std::{
    fs,
    io::{Read, Write},
    path::PathBuf,
    process::{ChildStdout, Command, ExitCode, Stdio},
    sync::atomic::{AtomicUsize, Ordering},
};

use clap::Parser;
use rayon::prelude::*;

/// Struct to store the command line args parsed by clap
#[derive(Parser, Debug)]
#[command(author="Mark Ross", version="0.1", about="Compiles and optionally runs the kernel", long_about = None)]
struct Args {
    /// Runs the kernel using qemu after compiling it.
    #[arg(long, action)]
    run: bool,

    /// Compiles the kernel in test mode and tests it.
    #[arg(long, action)]
    test: bool,

    /// Runs the kernel ready for a debugger to attach, with serial output written to the given file.
    /// Has no effect if not combined with --run.
    #[arg(long)]
    debug: Option<String>,

    /// Gets qemu to write a log file to the given file
    #[arg(long)]
    qemu_debug: Option<String>,

    /// Compiles the kernel in release mode.
    #[arg(long, action)]
    release: bool,

    /// Adds a device when running qemu.
    /// Has no effect if not combined with --run.  
    ///
    /// Example usage: `kernel-builder --run --qemu-device "pci-bridge,id=bridge0,chassis_nr=1"`
    #[arg(long)]
    qemu_device: Vec<String>,
}

/// This builder may be invoked with `pwd` = `project-root/kernel-builder` or just `project-root`.
/// This function computes the relative path to the `kernel` crate for either of these options.
fn kernel_dir() -> &'static str {
    if std::env::current_dir().unwrap().ends_with("kernel-builder") {
        "../kernel"
    } else {
        "kernel"
    }
}

/// Prepares a cargo command in the given directory, with the given subcommand
/// (e.g. if `subcommand` is `build`, `cargo build` will be run).
/// If `--release` is anywhere in this process's arguments, it will also be added to the subprocess's arguments.
fn prepare_cargo_command(args: &Args, dir: &str, subcommand: &str) -> Command {
    // Spawn a new cargo process to compile the kernel
    let mut cargo_process = std::process::Command::new("cargo");
    cargo_process.arg(subcommand).current_dir(dir);

    if args.release {
        if args.test {
            // This is a custom profile defined for the kernel which builds with optimisations and debug symbols
            cargo_process.arg("--profile=release-with-debug"); 
        } else {
            cargo_process.arg("--release");
        }
    }

    cargo_process
}

/// Prepares a call to the `qemu-system-x86-64` command.
///
/// # Arguments
/// * `file`: the file path to load as a disk image
/// * `test`: whether to run the kernel in test mode.
/// If `true`, a device will be added to allow the kernel to exit without usual power management, and no window will be shown.
fn prepare_qemu_command(args: &Args, file: &str, test: bool) -> Command {
    let mut c = std::process::Command::new("qemu-system-x86_64");

    c.arg("-bios").arg("/usr/share/ovmf/x64/OVMF.fd");

    c.arg("-machine").arg("q35");

    c.arg("-drive")
        .arg(format!("if=none,format=raw,id=os-drive,file={}", file)); // Load the specified image as a drive
    c.arg("-device").arg("qemu-xhci"); // Add an XHCI USB controller
    c.arg("-device").arg("usb-storage,drive=os-drive"); // Add the kernel image as a USB storage device
                                                        // c.arg("-device").arg("usb-mouse");
                                                        // c.arg("-device").arg("ps2-mouse");
    c.arg("-device").arg("pxb-pcie");

    if test {
        c.arg("-device")
            .arg("isa-debug-exit,iobase=0xf4,iosize=0x04")
            // .arg("-display")
            // .arg("none")
            .arg("-snapshot"); // Any writes to drives are discarded after the VM exits
    }

    if let Some(ref file) = args.debug {
        c.arg("-s") // Listen for debugger on port 1234
            .arg("-S") // Don't start until debugger gives command to
            .arg("-daemonize") // Run in background
            .arg("-serial")
            .arg(format!("file:{file}")); // Redirect serial to given file

        if let Some(ref qemu_file) = args.qemu_debug {
            c.arg("-D").arg(format!("{qemu_file}")).arg("-d").arg("int");
        }
    } else {
        c.arg("-serial").arg("stdio"); // Redirect serial to stdout
    }

    // Pass along other qemu args
    for arg in &args.qemu_device {
        c.arg("-device").arg(arg);
    }

    c
}

fn main() -> ExitCode {
    for (var, _) in std::env::vars() {
        if var.contains("CARGO") || var.contains("RUST") {
            std::env::remove_var(var);
        }
    }

    let args = &Args::parse();

    // If the --test flag is set, test the kernel instead
    if args.test {
        if args.run {
            panic!("--run and --test flags are mutually exclusive");
        }

        return run_tests(args);
    }

    let kernel_dir = kernel_dir();
    let mut cargo_process = prepare_cargo_command(args, kernel_dir, "build");

    let exit_code = cargo_process
        .spawn()
        .unwrap() // Spawn the process
        .wait()
        .unwrap() // Wait for the process to exit
        .code()
        .unwrap(); // Get the exit code

    // Check that cargo exited successfully
    assert_eq!(exit_code, 0);

    let kernel_path = if args.release {
        "target/x86_64-os/release/os"
    } else {
        "target/x86_64-os/debug/os"
    };

    let kernel = PathBuf::from(kernel_dir).join(kernel_path);

    let out_dir = PathBuf::from(kernel_dir).join("images");
    // Create the directory to put kernel images, if it doesn't exist.
    fs::create_dir_all(&out_dir).expect("Should have been able to create output directory");

    

    // create a BIOS disk image
    let bios_path = out_dir.join("bios.img");
    bootloader::BiosBoot::new(&kernel)
        .set_ramdisk(&kernel)
        .create_disk_image(&bios_path)
        .expect("Should have been able to create BIOS image");

    // create a BIOS disk image
    let uefi_path = out_dir.join("uefi.img");
    bootloader::UefiBoot::new(&kernel)
        .set_ramdisk(&kernel)           
        .create_disk_image(&uefi_path)
        .expect("Should have been able to create UEFI image");

    if args.release {
        cargo_process.arg("--release");
    }

    if args.run {
        prepare_qemu_command(args, uefi_path.to_str().unwrap(), false)
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
    }

    println!("{}", bios_path.to_str().unwrap());

    ExitCode::SUCCESS
}

/// Compiles the kernel in test mode and launches it for each test, recording the results.
///
/// In order to isolate different tests from each other, each one is run in a different VM instance.
/// This function first runs the kernel and queries it with how many tests there are, then runs each one individually.
/// Tests are run in parallel to speed up execution.
fn run_tests(args: &Args) -> ExitCode {
    let kernel_dir = kernel_dir();

    // Create a new cargo process to compile the kernel for tests
    let mut cargo_process = prepare_cargo_command(args, kernel_dir, "test");
    cargo_process.arg("--no-run").stderr(Stdio::piped());

    let mut cargo_process = cargo_process.spawn().unwrap();

    // Get a handle to the stdout of the cargo process
    let mut output = cargo_process.stderr.take().unwrap();

    // Read the stderr of the cargo process to a String
    let mut output_str = String::new();
    output.read_to_string(&mut output_str).unwrap();

    // Check that cargo exited successfully
    let exit_code = cargo_process.wait().unwrap().code().unwrap();
    if exit_code != 0 {
        println!("Cargo failed to compile test kernel:");
        println!("{output_str}");
        panic!();
    }

    // Extract the path to the test kernel
    let test_bin = output_str
        .split(' ')
        .last()
        .unwrap() // Get the full path in brackets
        .strip_prefix('(')
        .unwrap() // Strip the start bracket
        .strip_suffix(")\n")
        .unwrap(); // Strip the end bracket

    println!("Using test kernel binary at {test_bin}");

    // Parse the path to the test kernel
    let kernel = PathBuf::from(kernel_dir).join(test_bin);

    // create a BIOS disk image
    let uefi_path = kernel.parent().unwrap().join("uefi.img");
    bootloader::UefiBoot::new(&kernel)
        .create_disk_image(&uefi_path)
        .unwrap();

    // Run the kernel in qemu to ask it how many tests there are
    let (mut qemu_command, mut stdin, chars) = run_qemu_test(args, uefi_path.to_str().unwrap());

    // Send the 'count' command. The kernel should respond with a number of tests
    stdin
        .write_all(b"count\n")
        .expect("Failed to write to stdin");

    let output = chars.collect::<Vec<u8>>();
    let num_tests = std::str::from_utf8(&output)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    // Check that the test runner exited successfully
    // TODO: investigate why this isn't the same number as defined in the kernel
    assert_eq!(qemu_command.wait().unwrap().code().unwrap(), 33);

    // How many tests failed
    // This is atomic rather than just mutable because the following iterator is multi-threaded
    let failures = AtomicUsize::new(0);

    // Check each test in parallel
    (0..num_tests).into_par_iter().for_each(|i| {
        let (mut qemu_command, mut stdin, chars) = run_qemu_test(args, uefi_path.to_str().unwrap());

        // Send a 'run' command with the command number
        stdin
            .write_all(format!("run\n{i}\n").as_bytes())
            .expect("Failed to write to stdin");

        // Get the output of the rest of the kernel's execution so that it can be printed in case the test fails
        let output = chars.rest();

        // Extract the test name from the output
        let test_name: Vec<u8> = output.split(|c| *c == b'\n').next().unwrap().to_vec();
        let test_name = std::str::from_utf8(&test_name).unwrap().trim_end();

        // Check that the test runner exited successfully
        // TODO: investigate why this isn't the same number as defined in the kernel
        if qemu_command.wait().unwrap().code().unwrap() == 33 {
            // TODO: change these ANSI codes to something more portable
            println!("Running {test_name}... [\x1b[32mOK\x1b[0m]");
        } else {
            // If the test fails, print its output in yellow to be more obvious
            println!("{test_name}... [\x1b[31mERROR\x1b[0m]");
            println!("\x1b[31mSerial output of failed test:\x1b[0m");
            println!("\x1b[33m-----------------------------------");
            println!("{}", String::from_utf8_lossy(&output));
            println!("-----------------------------------\x1b[0m");

            failures.fetch_add(1, Ordering::Relaxed);
        }
    });

    let failures = failures.load(Ordering::Relaxed);

    println!(
        "\n{} out of {} tests completed successfully",
        num_tests - failures,
        num_tests
    );

    if failures != 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Launches the kernel in qemu from the image at the given path and waits for it to write a message to stdout
/// indicating it's listening for a test command.
fn run_qemu_test(
    args: &Args,
    uefi_path: &str,
) -> (
    std::process::Child,
    std::process::ChildStdin,
    ChildStdoutIter,
) {
    let mut qemu_command = prepare_qemu_command(args, uefi_path, true)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();

    // Get handles to stdout
    let stdout = qemu_command.stdout.take().expect("Failed to open stdout");
    let stdin = qemu_command.stdin.take().expect("Failed to open stdin");

    let mut chars = ChildStdoutIter::new(stdout);

    // Wait for the kernel to print the ready message
    'outer: loop {
        for c in b">>>>>> READY FOR TEST COMMAND\n" {
            if *c != chars.next().unwrap() {
                continue 'outer;
            }
        }

        break;
    }

    (qemu_command, stdin, chars)
}

/// A wrapper around a child process, which exposes an iterator over the process's stdout.
#[derive(Debug)]
struct ChildStdoutIter {
    /// The process whose output is being iterated
    process: ChildStdout,
    /// The buffered output
    buffer: [u8; 256],
    /// The current position in the buffer
    i: usize,
    /// The length of data in the buffer
    n: usize,
}

impl Iterator for ChildStdoutIter {
    type Item = u8;

    fn next(&mut self) -> Option<Self::Item> {
        if self.i < self.n {
            let v = self.buffer[self.i];
            self.i += 1;
            Some(v)
        } else {
            self.i = 1;
            self.n = self.process.read(&mut self.buffer).unwrap();
            if self.n == 0 {
                None
            } else {
                Some(self.buffer[0])
            }
        }
    }
}

impl ChildStdoutIter {
    /// Gets the rest of the process's output (that is, anything that's not been consumed by [`next`])
    ///
    /// [`next`]: ChildStdoutIter::next
    fn rest(mut self) -> Vec<u8> {
        let mut v = self.buffer[self.i..self.n].to_vec();

        self.process.read_to_end(&mut v).unwrap();

        v
    }

    /// Constructs a new [`ChildStdoutIter`]
    fn new(process: ChildStdout) -> Self {
        Self {
            process,
            buffer: [0; 256],
            i: 0,
            n: 0,
        }
    }
}
