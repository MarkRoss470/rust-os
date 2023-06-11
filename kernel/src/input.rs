use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use pc_keyboard::DecodedKey;

use crate::println;

/// A temporary global [`String`] to test the kernel heap allocator
static INPUT_BUFFER: OnceCell<ArrayQueue<DecodedKey>> = OnceCell::uninit();

pub fn init_keybuffer() {
    INPUT_BUFFER.init_once(|| ArrayQueue::new(1024));
}

pub fn push_key(key: DecodedKey) {
    if let Ok(buffer) = INPUT_BUFFER.try_get() {
        match buffer.push(key) {
            Ok(_) => (),
            Err(_) => println!("ERROR: Dropped input"),
        }
    } else {
        println!("ERROR: Input buffer not initialised");
    }
}

pub fn pop_key() -> Option<DecodedKey> {
    INPUT_BUFFER.try_get().ok()?.pop()
}