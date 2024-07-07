//! The [`bitfield_enum`] macro

/// Generates an `enum` which can be used as a field of a struct generated using the [`bitfield`] macro.
///
/// The code inside the macro will look like a normal rust `enum` definition, and will be copied mostly unchanged.
/// The output will also contain an implementation of two methods: `from_bits` and `into_bits`.
/// The enum definition may not contain variants with payloads, and each variant must be assigned a value
/// using the `#[value(...)]` attribute, which will identify its corresponding bit representation.
///
/// The last variant may have the `#[rest]` attribute instead, which will cause that attribute to be given a
/// field of the type specified. If a bit value is encountered which is not matched by any other variant,
/// this variant will be used instead with the bit value converted using an `as` conversion. This conversion is lossy,
/// so make sure that the type specified here is at least as bit as the number of bits this type is allocated within the bitfield.
/// If an enum without a `#[rest]` variant encounters a bit pattern with no corresponding variant, the
/// [`unreachable`] macro will be invoked.
///
/// The enum definition in the invocation must have a `#[bitfield_enum(...)]` attribute as the first attribute
/// (i.e. above any `derive` attributes), containing the integer type to use in the generated methods.
/// This should be the same type as given to the [`bitfield`] macro on the bitfield struct.
/// The `#[bitfield_enum(...)]` attribute can also contain a visibility, e.g. `#[bitfield(pub u32)]`.
/// This specifies the visibility of the generated methods, which are private by default.
///
/// # Example
///
/// ```
/// bitfield_enum!(
///     #[bitfield_enum(u8)]
///     pub enum BitfieldEnum {
///         #[value(0)]
///         Variant0,
///         #[value(1)]
///         Variant1,
///         #[value(2)]
///         Variant2,
///         #[rest]
///         Rest(u8),
///     }
/// );
///
/// bitfield_enum!(
///     #[bitfield_enum(pub u32)]
///     #[derive(Debug, Clone, Copy, PartialEq, Eq)]
///     pub enum AnotherBitfieldEnum {
///         #[value(0)]
///         Variant0,
///         #[value(10)]
///         Variant1,
///         #[value(0xFF)]
///         Variant2,
///         #[rest]
///         Rest(u8),
///     }
/// );
/// ```
macro_rules! bitfield_enum {
    (
        #[bitfield_enum($impl_vis: vis $base_type: ty)]
        $(#[$enum_attr: meta])* // Matches attributes on the enum
        $enum_vis: vis enum $enum_name: ident {
            // Match variants
            $(
                #[value($val: literal)]
                $(#[$variant_attr: meta])*
                $variant: ident,
            )+
            // Match last variant if present
            $(
                #[rest]
                $(#[$last_variant_attr: meta])*
                $last_variant: ident($last_variant_type: ty),
            )?
        }
    ) => {
        $(#[$enum_attr])*
        $enum_vis enum $enum_name {
            $(
                $(#[$variant_attr])*
                $variant,
            )+
            $(
                $(#[$last_variant_attr])*
                $last_variant($last_variant_type),
            )?
        }

        impl $enum_name {
            $impl_vis const fn from_bits(v: $base_type) -> Self {
                $crate::util::bitfield_enum::bitfield_enum!(
                    #match_from_bits,
                    $base_type,
                    v,
                    ($($variant = $val,)+),
                    $(($last_variant, $last_variant_type))?
                )
            }

            $impl_vis const fn into_bits(self) -> $base_type {
                $crate::util::bitfield_enum::bitfield_enum!(
                    #match_into_bits,
                    $base_type,
                    self,
                    ($($variant = $val,)+),
                    $(($last_variant, $last_variant_type))?
                )
            }
        }
    };

    // Two rules for generating the match statement in `from_bits`,
    // One for enums with a `#[rest]` variant and one for enums with no rest variant.
    (
        #match_from_bits,
        $base_type: ty,
        $match_var: expr,
        ($($variant: ident = $val: literal,)+),
    ) => {
        match $match_var {
            $(
                $val => Self::$variant,
            )+
            // If an enum has a variant for all values, this pattern can never be reached
            #[allow(unreachable_patterns)] 
            _ => unreachable!(),
        }
    };
    (
        #match_from_bits,
        $base_type: ty,
        $match_var: expr,
        ($($variant: ident = $val: literal,)+),
        ($last_variant: ident, $last_variant_type: ty)
    ) => {
        match $match_var {
            $(
                $val => Self::$variant,
            )+
            bits => {
                #[allow(clippy::cast_possible_truncation)]
                Self::$last_variant(bits as $last_variant_type)
            },
        }
    };

    // Two rules for generating the match statement in `into_bits`,
    // One for enums with a `#[rest]` variant and one for enums with no rest variant.
    (
        #match_into_bits,
        $base_type: ty,
        $match_var: expr,
        ($($variant: ident = $val: literal,)+),
    ) => {
        match $match_var {
            $(
                Self::$variant => $val,
            )+
        }
    };
    (
        #match_into_bits,
        $base_type: ty,
        $match_var: expr,
        ($($variant: ident = $val: literal,)+),
        ($last_variant: ident, $last_variant_type: ty)
    ) => {
        match $match_var {
            $(
                Self::$variant => $val,
            )+
            Self::$last_variant(bits) => bits as $base_type,
        }
    };

}

pub(crate) use bitfield_enum;

#[test_case]
fn test_bitfield_enum() {
    bitfield_enum!(
        #[bitfield_enum(pub u32)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum BitfieldEnum {
            #[value(0)]
            /// A doc comment
            Variant0,
            #[value(10)]
            /// Another doc comment
            Variant1,
            #[value(0xFF)]
            Variant2,
            #[rest]
            Rest(u8),
        }
    );

    assert_eq!(BitfieldEnum::from_bits(0xff), BitfieldEnum::Variant2);
    assert_eq!(BitfieldEnum::from_bits(0xfe), BitfieldEnum::Rest(0xfe));
    assert_eq!(BitfieldEnum::Variant1.into_bits(), 10);
    assert_eq!(BitfieldEnum::Rest(40).into_bits(), 40);
}