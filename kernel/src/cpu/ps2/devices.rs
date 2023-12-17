//! Drivers for PS/2 devices

use core::fmt::Debug;

use pc_keyboard::{layouts, HandleControl, Keyboard, ScancodeSet2};

use crate::input::push_key;

use super::{Ps2ControllerInitialisationError, Ps2Port, Ps2Ports};

/// A device which is connected to a PS/2 port
pub(super) enum Ps2Device {
    /// An AT keyboard
    ATKeyboard,
    /// A standard 3-button mouse
    StandardMouse,
    /// A mouse which has a scroll wheel
    MouseWithScrollWheel,
    /// A 5-button mouse
    FiveButtonMouse,
    /// An Mf2 keyboard
    MF2Keyboard(Mf2Keyboard),
    /// A short (i.e. not full size) keyboard
    ShortKeyboard,
    /// An unknown or unimplemented device
    Unknown,
}

impl Debug for Ps2Device {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::ATKeyboard => write!(f, "ATKeyboard"),
            Self::StandardMouse => write!(f, "StandardMouse"),
            Self::MouseWithScrollWheel => write!(f, "MouseWithScrollWheel"),
            Self::FiveButtonMouse => write!(f, "FiveButtonMouse"),
            Self::MF2Keyboard(_) => write!(f, "MF2Keyboard"),
            Self::ShortKeyboard => write!(f, "ShortKeyboard"),
            Self::Unknown => write!(f, "Unknown"),
        }
    }
}

impl Ps2Device {
    /// Constructs a new keyboard device
    pub const fn new_keyboard() -> Self {
        Self::MF2Keyboard(Mf2Keyboard::new())
    }

    /// Initialises the device on the given port.
    pub unsafe fn init(
        &mut self,
        port: Ps2Port,
        ports: &mut Ps2Ports,
    ) -> Result<(), Ps2ControllerInitialisationError> {
        if let Self::StandardMouse = self {
            // SAFETY: This command will activate the mouse
            unsafe { ports.port_send_command(port, super::Ps2DeviceCommand::EnableScanning)? };
        }

        Ok(())
    }

    /// Reads from the port and parses and acts upon the data received.
    ///
    /// # Safety
    /// This device must be plugged in on the given port.
    /// This method should be called from the interrupt handler for that port.
    pub unsafe fn poll(&mut self, port: Ps2Port, ports: &mut Ps2Ports) {
        // SAFETY: This method should only be called from an interrupt handler
        unsafe {
            match self {
                Self::MF2Keyboard(k) => k.poll(port, ports),
                Self::StandardMouse => {
                    let Some(packet) = ports.read() else {
                        return;
                    };
                }
                _ => todo!(),
            }
        }
    }
}

/// An Mf2 keyboard device
pub(super) struct Mf2Keyboard(Keyboard<layouts::Us104Key, ScancodeSet2>);

impl Mf2Keyboard {
    /// Constructs a new [`Mf2Keyboard`] in a default state
    const fn new() -> Self {
        Self(Keyboard::new(
            ScancodeSet2::new(),
            layouts::Us104Key,
            HandleControl::Ignore,
        ))
    }

    /// Polls the keyboard for keypresses
    ///
    /// # Safety
    /// As this function does not check that any read data comes from the keyboard,
    /// it should only be called from the interrupt handler for the keyboard's PS/2 port.
    unsafe fn poll(&mut self, _port: Ps2Port, ports: &mut Ps2Ports) {
        // SAFETY: This is called from an interrupt handler which means any data comes from this device
        let Some(scancode) = (unsafe { ports.read() }) else {
            return;
        };

        // Parse the scancode using the pc-keyboard crate
        if let Ok(Some(key_event)) = self.0.add_byte(scancode) {
            if let Some(key) = self.0.process_keyevent(key_event) {
                push_key(key);
            }
        }
    }
}
