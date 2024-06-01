//! The [`Mutability`] trait and [`Mutable`] and [`Immutable`] marker types for being generic over mutability

/// An abstract trait representing either mutable or immutable pointers and references.
/// This can be used to make structs generic over whether they hold mutable or immutable references and pointers to data.
///
/// For example, instead of needing to write the following:
///
/// ```
/// struct SomeDataStructure<'a>(&'a u8);
///
/// impl<'a> SomeDataStructure<'a> {
///     fn read(&self) -> u8 {
///         *self.0
///     }
/// }
///
/// struct SomeDataStructureMut<'a>(&'a mut u8);
///
/// impl<'a> SomeDataStructureMut<'a> {
///     fn read(&self) -> u8 {
///         *self.0
///     }
///
///     fn write(&mut self, x: u8) {
///         *self.0 = x;
///     }
/// }
/// ```
///
/// The structs `SomeDataStructure` and `SomeDataStructureMut` can be combined into the following:
///
/// ```
// NOTE: when updating this code, update the test at the bottom of the file as well
///
///     struct SomeDataStructure<'a, M: Mutability>(M::Ref<'a, u8>);
///
///     impl<'a, M: Mutability> SomeDataStructure<'a, M> {
///         fn read(&self) -> u8 {
///             // A conversion method is used to cast the opaque reference type to a shared reference
///             *self.0.as_const_ref()
///         }
///     }
///
///     impl<'a> SomeDataStructure<'a, Mutable> {
///         fn write(&mut self, x: u8) {
///             // No conversion is needed here because `M` is definitely `Mutable`,
///             // which means that the reference type is already inferred to be `&mut u8`
///             *self.0 = x;
///         }
///     }
/// ```
pub trait Mutability {
    /// The reference type for this mutability
    type Ref<'a, T: 'a>: Reference<'a, T>;
    /// The pointer type for this mutability
    type Ptr<T>: Pointer<T>;
}

/// Represents mutable references and pointers, for use with the [`Mutability`] trait.
pub struct Mutable;

impl Mutability for Mutable {
    type Ref<'a, T: 'a> = &'a mut T;
    type Ptr<T> = *mut T;
}

/// Represents immutable references and pointers, for use with the [`Mutability`] trait.
pub struct Immutable;

impl Mutability for Immutable {
    type Ref<'a, T: 'a> = &'a T;
    type Ptr<T> = *const T;
}

/// A trait which any pointer can implement.
/// Methods specific to mutable pointers are not included in this trait - to use methods on mutable pointers,
/// only make a method available for [`Mutable`] pointers. Then [`Ptr`] will resolve to `*mut T` and mutable pointer methods are available.
///
/// [`Ptr`]: Mutability::Ptr
pub trait Pointer<T>: Copy {
    /// Converts to a `*const T`.
    ///
    /// Any pointer type can be cast to a `const` pointer, so this method is available for any type
    /// implementing [`Pointer`].
    fn as_const_ptr(self) -> *const T;
}

impl<T> Pointer<T> for *const T {
    fn as_const_ptr(self) -> *const T {
        self
    }
}

impl<T> Pointer<T> for *mut T {
    fn as_const_ptr(self) -> *const T {
        self
    }
}

/// A trait which any reference can implement.
/// Methods specific to mutable references are not included in this trait - to use methods on mutable references,
/// only make a method available for [`Mutable`] references. Then [`Ref`] will resolve to `&mut T` and mutable reference methods are available.
///
/// [`Ref`]: Mutability::Ref
pub trait Reference<'a, T> {
    /// Converts to a `*const T`.
    ///
    /// Any pointer type can be cast to a `const` pointer, so this method is available for any type
    /// implementing [`Pointer`].
    fn as_const_ref<'b>(&'b self) -> &'b T
    where
        'a: 'b;
}

impl<'a, T> Reference<'a, T> for &'a T {
    fn as_const_ref<'b>(&'b self) -> &'b T
    where
        'a: 'b,
    {
        self
    }
}

impl<'a, T> Reference<'a, T> for &'a mut T {
    fn as_const_ref<'b>(&'b self) -> &'b T
    where
        'a: 'b,
    {
        self
    }
}

#[cfg(test)]
mod tests {
    use core::marker::PhantomData;

    use super::*;

    #[test_case]
    #[allow(dead_code)]
    fn test_generic_refs() {
        struct SomeDataStructure<'a, M: Mutability>(M::Ref<'a, u8>);

        impl<'a, M: Mutability> SomeDataStructure<'a, M> {
            fn read(&self) -> u8 {
                // A conversion method is used to cast the opaque reference type to a shared reference
                *self.0.as_const_ref()
            }
        }

        impl<'a> SomeDataStructure<'a, Mutable> {
            fn write(&mut self, x: u8) {
                // No conversion is needed here because `M` is definitely `Mutable`,
                // which means that the reference type is already inferred to be `&mut u8`
                *self.0 = x;
            }
        }
    }

    #[test_case]
    #[allow(dead_code)]
    fn test_generic_pointers() {
        struct SomeDataStructure<'a, M: Mutability>(M::Ptr<u8>, PhantomData<&'a u8>);

        impl<'a, M: Mutability> SomeDataStructure<'a, M> {
            fn read(&self) -> u8 {
                // SAFETY: For tests only
                unsafe { *self.0.as_const_ptr() }
            }
        }

        impl<'a> SomeDataStructure<'a, Mutable> {
            fn write(&mut self, x: u8) {
                // SAFETY: For tests only
                unsafe { *self.0 = x };
            }
        }
    }
}
