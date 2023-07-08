use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use pc_keyboard::DecodedKey;

use crate::println;

/// A buffer of keyboard inputs. An input will be added to this buffer when a key is pressed, 
/// and removed when it is read by an input handler.
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