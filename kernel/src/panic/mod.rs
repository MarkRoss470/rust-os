//! Code related to panicking

#[cfg(debug_assertions)]
pub mod backtrace;

/// This function is called on panic.
#[cfg(not(test))]
#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    use super::cpu::gdt::get_stack;
    use super::graphics::flush;
    use crate::println;

    x86_64::instructions::interrupts::disable();

    println!("{info}");

    let stack_pointer_approx = info as *const _ as usize;

    println!(
        "Current stack pointer is approximately {:#x}",
        stack_pointer_approx
    );

    println!("In stack {:?}", get_stack(stack_pointer_approx));

    #[cfg(debug_assertions)]
    match backtrace::backtrace() {
        Ok(_) => (),
        Err(e) => println!("Error printing backtrace: {e:?}"),
    }

    // There's no nice way to handle this because unwrapping would cause a second panic,
    // while just printing an error would require a second call to `flush`.
    // The best thing to do is just ignore the error.
    let _ = flush();

    loop {
        x86_64::instructions::hlt();
    }
}
