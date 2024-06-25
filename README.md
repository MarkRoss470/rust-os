# Rust OS (working title)

An experimental operating system written in Rust, using the bootloader crate for interacting with UEFI.

## Getting Started

The project is split into two crates. The kernel code is in `kernel/`, and the code to build, package, and optionally run the kernel is in `kernel-builder/`. To build the kernel, `cd` into `kernel-builder/` and run `cargo run`. This will build the kernel, and the kernel images will be generated at `kernel/images/`. These images can be copied to a USB drive with `dd` to run on real hardware, or run with qemu.

I have only tested building the kernel on linux. Building the kernel also needs some commands to be present on the host system, such as `cc`, `objcopy`, and some other utilities. The kernel runner and tester also require `qemu-system-x86_64`.

### Running under qemu

The OS only supports booting from UEFI, so running under qemu requires the OVMF firmware to provide UEFI functionality. The path to this firmware must be passed to `kernel-builder` using the `--bios-path` flag in order to run or test the kernel.

Run `cargo run -- --run --bios-path=<your OVMF path>` while in the `kernel-builder` directory to run the kernel in qemu after building it (note the `--` to separate the argument to cargo from the arguments to the kernel builder). To see a list of all possible arguments, run `cargo run -- --help`.

### VSCode launch scripts

If you are using vscode, the config files in `.vscode` set up a launch config to debug the kernel. This requires the "Native Debug" extension for vscode. The scripts get the path to OVMF firmware from the shell file at `.vscode/ovmf.sh` - edit this file to point to the right place.

## Features

The OS currently has very few user-facing features, as I am working on hardware support (e.g. PCI, USB) before things like processes and syscalls.

Current features:  
 - Basic software text rendering for `print!` and `println!`
 - Keyboard input using interrupts (this relies on an emulated PS/2 keyboard, so it won't work on all hardware)
 - Kernel heap allocator
 - Basic PCI device enumeration support
 - Basic ACPI support using Intel's ACPICA library, including:
   - Enumerating devices
   - Powering off the system

Current development features:
 - Automated test runner using qemu
 - Ability to redirect kernel logs/output to a file when running in qemu
 - Stack backtraces for kernel panics when running in debug mode

## Features In Development

- Further ACPI support using the ACPICA C library. I am writing my own rust bindings to this library as no existing bindings exist. The source code for these bindings are [here](https://github.com/MarkRoss470/acpica-rust-bindings).
- XHCI support for interacting with USB devices. The kernel can currently enumerate XHCI controllers and send no-op packets, but not exchange data with USB devices.

## Screenshots

Enumerating PCI devices:

![The operating system running under qemu. The screen is mostly black but with some white text showing the PCI devices connected to the virtual machine.](images/lspci.png)

Enumerating ACPI devices:

![The operating system running under qemu. The screen shows many lines of text showing the virtual devices exposed by AML code.](images/enumerating-acpi-devices.png)

Stack backtraces:
![The operating system running under qemu. The screen shows the user entering the 'panic' command, followed by the kernel's panic output. The output contains a stack backtrace, with each frame containing the instruction pointer, function name, and the source file and line.](images/stack-backtraces.png)

## Credits

 * JDH, whose [Tetris OS](https://www.youtube.com/watch?v=FaILnmUYS_U) inspired me to look into operating systems
 * Philipp Oppermann, whose [blog](https://os.phil-opp.com/) formed the basis of this project.