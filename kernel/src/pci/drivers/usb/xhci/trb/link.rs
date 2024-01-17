//! The [`LinkTrb`] type

use x86_64::PhysAddr;

use super::TrbType;

#[bitfield(u32)]
pub struct LinkTrbConfig {
    #[bits(22)]
    _reserved: (),

    /// The interrupter to send a [`CommandCompletionTrb`] to when the TRB is processed.
    /// An interrupt is only sent if [`interrupt_on_completion`] is `true`
    ///
    /// [`CommandCompletionTrb`]: super::event::command_completion::CommandCompletionTrb
    /// [`interrupt_on_completion`]: LinkTrbFlags::interrupt_on_completion
    #[bits(10)]
    pub interrupter_target: u16,
}

#[bitfield(u32)]
pub struct LinkTrbFlags {
    /// The cycle bit
    pub cycle: bool,
    /// Whether the controller should switch its cycle state after this TRB
    pub toggle_cycle: bool,

    #[bits(2)]
    _reserved: (),

    /// Whether the next TRB is part of the same TD
    pub chain: bool,
    /// Whether an interrupt should be sent to the interrupter indicated by [`interrupter_target`] when the TRB is completed
    ///
    /// [`interrupter_target`]: LinkTrbConfig::interrupter_target
    pub interrupt_on_completion: bool,

    #[bits(4)]
    _reserved: (),

    #[bits(6, default = TrbType::Link)]
    pub trb_type: TrbType,

    #[bits(16)]
    _reserved: (),
}

/// A TRB on a [control] or transfer (TODO: link) TRB ring which indicates the end of a segment of the ring.
/// The TRB also contains the [`toggle_cycle`] flag, which tells whether controller to switch its cycle state.
/// This is so that the controller doesn't have to overwrite TRBs after it reads them - TRBs which were written
/// on the first pass around the ring will not be read on the controller's next pass because the cycle bit will not match
/// the controller's cycle state.
///
/// [Control]: super::command_ring::CommandTrbRing
/// [`toggle_cycle`]: LinkTrbFlags::cycle
#[derive(Debug)]
pub struct LinkTrb {
    /// The start address of the new ring segment
    pub pointer: PhysAddr,
    /// Configuration for the TRB
    pub config: LinkTrbConfig,
    /// The TRB flags
    pub flags: LinkTrbFlags,
}

impl LinkTrb {
    /// Converts the TRB to the data written to a TRB ring
    pub fn to_parts(&self, cycle: bool) -> [u32; 4] {
        assert!(self.pointer.is_aligned(16u64));

        let pointer = self.pointer.as_u64();
        let config = self.config.into();
        let flags = self.flags.with_cycle(cycle).into();

        #[allow(clippy::cast_possible_truncation)]
        [pointer as u32, (pointer >> 32) as u32, config, flags]
    }

    /// Constructs
    pub fn new(addr: PhysAddr, cycle: bool, toggle_cycle: bool, chain: bool) -> Self {
        Self {
            pointer: addr,
            config: LinkTrbConfig::new(), // TODO: interrupter target
            flags: LinkTrbFlags::new()
                .with_cycle(cycle)
                .with_toggle_cycle(toggle_cycle)
                .with_chain(chain)
                .with_interrupt_on_completion(true),
        }
    }

    /// Gets the value of the chain bit
    pub fn chain(&self) -> bool {
        self.flags.chain()
    }
}
