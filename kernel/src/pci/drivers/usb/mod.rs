//! Drivers for USB controllers

use core::fmt::Debug;

pub mod xhci;

/// A USB route string. This uniquely identifies a connected USB device on a root port by which port it is plugged into on a hub,
/// which port that hub is plugged into, etc.
///
/// This data structure is defined in section 8.9 of the [USB3 specification].
///
/// [USB3 specification]: https://www.usb.org/document-library/usb-32-revision-11-june-2022
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct RouteString(u32);

impl RouteString {
    /// Gets the offset at the given `tier`. The `tier` is 1-based and doesn't include the root hub.
    /// For example, a `tier` value of 1 will return the port number on the hub directly connected to the root hub.
    ///
    /// The return value is always in the range `0..=15`
    ///
    /// # Panics
    /// * If `tier == 0`
    /// * If `tier > 5`
    pub fn offset_at_tier(&self, tier: u8) -> u8 {
        assert!(tier <= 5);
        assert!(tier != 0, "tier is 1-based");

        (self.0 >> (4 * (tier - 1)) & 0b1111) as u8
    }

    /// Gets an iterator over the port offsets, starting at the hub closest to the root hub.
    pub fn offsets(&self) -> impl Iterator<Item = u8> {
        let copy = *self;
        (1..=5).map(move |i| copy.offset_at_tier(i))
    }
}

impl Debug for RouteString {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.offsets()).finish()
    }
}

/// Methods for using the [`RouteString`] as part of a bitfield struct using the [`bitfield`] macro.
impl RouteString {
    /// Constructs a [`RouteString`] from its bit representation
    const fn from_bits(bits: u32) -> Self {
        assert!(bits >> 20 == 0, "Only the bottom 20 bits may be set");

        Self(bits)
    }

    /// Converts a [`RouteString`] into its bit representation
    const fn into_bits(self) -> u32 {
        self.0
    }
}
