use std::path::PathBuf;

fn main() {
    let exit_code = std::process::Command::new("cargo")
        .arg("build")
        .current_dir("kernel")
        .spawn()
        .unwrap()
        .wait()
        .unwrap()
        .code()
        .unwrap();

    assert_eq!(exit_code, 0);

    let out_dir = PathBuf::from("kernel/target/x86_64-os/debug");
    let kernel = out_dir.join("os");

    // create a BIOS disk image
    let bios_path = out_dir.join("bios.img");
    bootloader::BiosBoot::new(&kernel)
        .create_disk_image(&bios_path).unwrap();

    std::process::Command::new("qemu-system-x86_64")
        .arg("-drive").arg(format!("format=raw,file={}",bios_path.to_str().unwrap()))
        .spawn()
        .unwrap()
        .wait()
        .unwrap();

}
