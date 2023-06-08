use std::{io::Read, path::PathBuf, process::{Stdio, Command}};

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
fn prepare_cargo_command(dir: &str, subcommand: &str) -> Command {
    // Spawn a new cargo process to compile the kernel
    let mut cargo_process = std::process::Command::new("cargo");
    cargo_process.arg(subcommand).current_dir(dir);

    for arg in std::env::args().skip(1) {
        if arg == "--release" {
            cargo_process.arg("--release");
        }
    }

    cargo_process
}

fn main() {
    let kernel_dir = kernel_dir();
    let mut cargo_process = prepare_cargo_command(kernel_dir, "build");

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

    std::process::Command::new("qemu-system-x86_64")
        .arg("-drive")
        .arg(format!("format=raw,file={}", bios_path.to_str().unwrap()))
        .arg("-serial")
        .arg("stdio") // Redirect serial to stdout
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
}

/// Compiles the kernel in test mode and launches it
#[test]
fn run_tests() {
    let kernel_dir = kernel_dir();

    // Create a new cargo process to compile the kernel for tests
    let mut cargo_process = prepare_cargo_command(kernel_dir, "test");
    cargo_process
        .arg("--no-run")
        .stderr(Stdio::piped());

    let mut cargo_process = cargo_process.spawn().unwrap();

    // Get a handle to the stdout of the cargo process
    let mut output = cargo_process.stderr.take().unwrap();

    // Read the stderr of the cargo process to a String
    let mut output_str = String::new();
    output.read_to_string(&mut output_str);

    // Check that cargo exited successfully
    let exit_code = cargo_process.wait().unwrap().code().unwrap();
    if exit_code != 0 {
        println!("Cargo failed to compile test kernel:");
        println!("{output_str}");
        panic!();
    }

    // Extract the path to the test kernel
    let test_bin = output_str
        .split(" ")
        .last()
        .unwrap() // Get the full path in brackets
        .strip_prefix("(")
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

    // Launch qemu running the test kernel
    let qemu_exit_code = std::process::Command::new("qemu-system-x86_64")
        .arg("-drive")
        .arg(format!("format=raw,file={}", bios_path.to_str().unwrap())) // Load test image
        .arg("-device")
        .arg("isa-debug-exit,iobase=0xf4,iosize=0x04") // Add fake device to allow exit with a status code
        .arg("-serial")
        .arg("stdio") // Redirect serial to stdout
        .arg("-display")
        .arg("none") // Don't display a window
        .spawn()
        .unwrap() // Spawn the process
        .wait()
        .unwrap() // Wait for the process to exit
        .code()
        .unwrap(); // Get the exit code

    // Check that the test runner exited successfully
    // TODO: investigate why this isn't the same number as defined in the kernel
    assert_eq!(qemu_exit_code, 33);
}
