//! Types for various context types

pub mod device_context;
pub mod endpoint_context;
pub mod input_context;
pub mod slot_context;

/// The size of various context data structures.
///
/// This is dependant on the [`context_size`] field of the controller's capability registers.
/// 
/// [`context_size`]: super::capability_registers::CapabilityParameters1::context_size
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextSize {
    /// A context struct takes 32 bytes
    Small,
    /// A context struct takes 64 bytes
    Large,
}

impl ContextSize {
    /// Gets the number of bytes in a context structure
    fn bytes(self) -> usize {
        match self {
            Self::Small => 32,
            Self::Large => 64,
        }
    }
}
