#!/bin/sh

source .vscode/ovmf.sh

# Move to the kernel-builder directory
cd $1/../kernel-builder

# Run the kernel-builder
# Replace `$QEMU_UEFI_PATH` on the next line with the path to the OVMF firmware
cargo run -- --test --release  --bios-path=$VSCODE_QEMU_UEFI_PATH 