//! Device management code

use crate::global_state::KERNEL_STATE;

/// # Safety
/// This function must only be called once
pub unsafe fn init() {
    let acpica = KERNEL_STATE.acpica.lock();
    let _: Option<()> = acpica.scan_devices(|handle, _| {
        let info = handle.get_info().unwrap();

        // debug!(
        //     "{}: {:?} {:?} {:?} {:?}",
        //     handle.path().unwrap(),
        //     info.hardware_id(),
        //     info.unique_id(),
        //     info.class_code(),
        //     crate::util::iterator_list_debug::IteratorListDebug::new(info.compatible_id_list())
        // );

        None
    });
}
