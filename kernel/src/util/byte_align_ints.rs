//! Contains the `ByteAlignU_` and `ByteAlignI_` types for representing integers but without the alignment requirements

use core::fmt::Debug;

/// Declares a type which behaves like an int type but has an align of 1 byte
macro_rules! byte_align_int {
    (
        $type_name: ident, $num_bytes: literal, $wrapped_type: ty,
        $from_method_name: ident, $to_method_name: ident
    ) => {
        /// A type which stores a [`
        #[doc = stringify!($wrapped_type)]
        /// `] in little-endian format, but has an align of 1.
        /// This is useful for handling data structures with fields which are not aligned.
        #[derive(Clone, Copy, PartialEq, Eq)]
        #[repr(transparent)]
        pub struct $type_name([u8; $num_bytes]);

        impl $type_name {
            /// Converts the [`
            #[doc = stringify!($wrapped_type)]
            /// `] to a [`
            #[doc = stringify!($type_name)]
            /// `]
            pub fn $from_method_name(value: $wrapped_type) -> Self {
                Self(value.to_le_bytes())
            }

            /// Converts the [`
            #[doc = stringify!($type_name)]
            /// `] to a [`
            #[doc = stringify!($wrapped_type)]
            /// `]
            pub fn $to_method_name(self) -> $wrapped_type {
                <$wrapped_type>::from_le_bytes(self.0)
            }
        }

        impl From<$wrapped_type> for $type_name {
            fn from(value: $wrapped_type) -> Self {
                Self::$from_method_name(value)
            }
        }

        impl From<$type_name> for $wrapped_type {
            fn from(value: $type_name) -> Self {
                <$type_name>::$to_method_name(value)
            }
        }

        impl PartialOrd for $type_name {
            fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
                Some(self.$to_method_name().cmp(&other.$to_method_name()))
            }
        }

        impl Ord for $type_name {
            fn cmp(&self, other: &Self) -> core::cmp::Ordering {
                self.$to_method_name().cmp(&other.$to_method_name())
            }
        }

        impl Debug for $type_name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_tuple(stringify!($type_name))
                    .field(&self.$to_method_name())
                    .finish()
            }
        }
    };
}

// No u8 or i8 because they are already byte-aligned

byte_align_int!(ByteAlignU16, 2, u16, from_u16, to_u16);
byte_align_int!(ByteAlignU32, 4, u32, from_u32, to_u32);
byte_align_int!(ByteAlignU64, 8, u64, from_u64, to_u64);
byte_align_int!(ByteAlignU128, 16, u128, from_u128, to_u128);

byte_align_int!(ByteAlignI16, 2, i16, from_i16, to_i16);
byte_align_int!(ByteAlignI32, 4, i32, from_i32, to_i32);
byte_align_int!(ByteAlignI64, 8, i64, from_i64, to_i64);
byte_align_int!(ByteAlignI128, 16, i128, from_i128, to_i128);
