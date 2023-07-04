use std::{path::PathBuf, process::Command, ffi::{OsString, OsStr}};

use clap::Parser;

/// Struct to store the command line args parsed by clap
#[derive(Parser, Debug)]
#[command(author="Mark Ross", version="0.1", about="Compiles and optionally runs the kernel", long_about = None)]
struct Args {
    /// Runs the kernel using qemu after compiling it.
    #[arg(long, action)]
    run: bool,

    /// Runs the kernel ready for a debugger to attach, with serial output written to the given file.
    /// Has no effect if not combined with --run
    #[arg(long)]
    debug: Option<String>,

    /// Compiles the kernel in release mode.
    #[arg(long, action)]
    release: bool,
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
        cargo_process.arg("--release");
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

    c.arg("-drive").arg(format!("if=none,format=raw,id=os-drive,file={}", file)); // Load the specified image as a drive
    c.arg("-device").arg("qemu-xhci"); // Add an XHCI USB controller
    c.arg("-device").arg("usb-storage,drive=os-drive"); // Add the kernel image as a USB storage device

    if test {
        c.arg("-device")
            .arg("isa-debug-exit,iobase=0xf4,iosize=0x04")
            .arg("-display")
            .arg("none");
    }

    if let Some(ref file) = args.debug {
        c.arg("-s") // Listen for debugger on port 1234
            .arg("-S") // Don't start until debugger gives command to
            .arg("-daemonize") // Run in background
            .arg("-serial")
            .arg(format!("file:{file}")); // Redirect serial to given file
    } else {
        c.arg("-serial").arg("stdio"); // Redirect serial to stdout
    }

    c
}

fn main() {
    let args = &Args::parse();

    for (var, _) in std::env::vars() {
        if var.contains("CARGO") || var.contains("RUST") {
            std::env::remove_var(var);
        }
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

    let out_dir = PathBuf::from(kernel_dir).join("target/x86_64-os/debug");
    let kernel = out_dir.join("os");

    // create a BIOS disk image
    let bios_path = out_dir.join("bios.img");
    bootloader::BiosBoot::new(&kernel)
        .create_disk_image(&bios_path)
        .unwrap();

    // create a BIOS disk image
    let uefi_path = out_dir.join("uefi.img");
    bootloader::UefiBoot::new(&kernel)
        .create_disk_image(&uefi_path)
        .unwrap();

    if args.release {
        cargo_process.arg("--release");
    }

    if args.run {
        prepare_qemu_command(args, bios_path.to_str().unwrap(), false)
            .spawn()
            .unwrap()
            .wait()
            .unwrap();
    }

    println!("{}", bios_path.to_str().unwrap());
}

/// Compiles the kernel in test mode and launches it
#[test]
fn run_tests() {
    use std::process::Stdio;
    use std::io::Read;

    let args = &Args::parse();

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

    // Parse the path to the test kernel
    let kernel = PathBuf::from(kernel_dir).join(test_bin);

    // create a BIOS disk image
    let bios_path = kernel.parent().unwrap().join("bios.img");
    bootloader::BiosBoot::new(&kernel)
        .create_disk_image(&bios_path)
        .unwrap();

    let qemu_exit_code = prepare_qemu_command(args, bios_path.to_str().unwrap(), true)
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .code()
        .unwrap();

    // Check that the test runner exited successfully
    // TODO: investigate why this isn't the same number as defined in the kernel
    assert_eq!(qemu_exit_code, 33);
}
