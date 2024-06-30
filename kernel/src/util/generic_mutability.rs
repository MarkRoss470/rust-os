//! The [`Mutability`] trait and [`Mutable`] and [`Immutable`] marker types for being generic over mutability

use core::fmt::Debug;

use x86_64::VirtAddr;

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
    type Ptr<T>: Pointer<T, M = Self>;
}

/// Represents mutable references and pointers, for use with the [`Mutability`] trait.
#[derive(Debug)]
pub struct Mutable;

impl Mutability for Mutable {
    type Ref<'a, T: 'a> = &'a mut T;
    type Ptr<T> = *mut T;
}

/// Represents immutable references and pointers, for use with the [`Mutability`] trait.
#[derive(Debug)]
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
#[allow(clippy::missing_safety_doc)]
pub trait Pointer<T>: Copy + Debug{
    /// The mutability of this pointer
    type M: Mutability;

    /// Converts to a `*const T`.
    ///
    /// Any pointer type can be cast to a `const` pointer, so this method is available for any type
    /// implementing [`Pointer`].
    fn as_const_ptr(self) -> *const T;

    /// Converts from a usize to a ptr
    fn from_usize(p: usize) -> Self;

    // Methods with the same signatures as those on raw pointers.
    // This is so that the methods can be used on generic pointers as if they were raw pointers
    // Methods which can be used by casting to a const pointer first are not included
    /// See `<*const T>::add` and `<*mut T>::add`
    unsafe fn add(self, count: usize) -> Self;
    /// See `<*const T>::cast` and `<*mut T>::cast`
    fn cast<U>(self) -> <Self::M as Mutability>::Ptr<U>;
}

impl<T> Pointer<T> for *const T {
    type M = Immutable;
    
    fn as_const_ptr(self) -> *const T {
        self
    }

    fn from_usize(p: usize) -> Self {
        p as Self
    }

    unsafe fn add(self, count: usize) -> Self {
        // SAFETY: Same as caller
        unsafe { self.add(count) }
    }
    
    fn cast<U>(self) -> <Self::M as Mutability>::Ptr<U> {
        self.cast()
    }
}

impl<T> Pointer<T> for *mut T {
    type M = Mutable;

    fn as_const_ptr(self) -> *const T {
        self
    }

    fn from_usize(p: usize) -> Self {
        p as Self
    }

    unsafe fn add(self, count: usize) -> Self {
        // SAFETY: Same as caller
        unsafe { self.add(count) }
    }
    
    fn cast<U>(self) -> <Self::M as Mutability>::Ptr<U> {
        self.cast()
    }
}

/// A trait which any reference can implement.
/// Methods specific to mutable references are not included in this trait - to use methods on mutable references,
/// only make a method available for [`Mutable`] references. Then [`Ref`] will resolve to `&mut T` and mutable reference methods are available.
///
/// [`Ref`]: Mutability::Ref
pub trait Reference<'a, T> {
    /// Converts to an immutable reference.
    fn as_const_ref<'b>(&'b self) -> &'b T
    where
        'a: 'b;

    /// Converts from a mutable reference to this type of reference
    fn from_mut_ref(r: &'a mut T) -> Self;

}

impl<'a, T> Reference<'a, T> for &'a T {
    fn as_const_ref<'b>(&'b self) -> &'b T
    where
        'a: 'b,
    {
        self
    }
    
    fn from_mut_ref(r: &'a mut T) -> Self {
        r
    }
    
}

impl<'a, T> Reference<'a, T> for &'a mut T {
    fn as_const_ref<'b>(&'b self) -> &'b T
    where
        'a: 'b,
    {
        self
    }
    
    fn from_mut_ref(r: &'a mut T) -> Self {
        r
    }
}

/// A trait which can be used as an additional bound on top of [`Mutability`] to require that
/// a [`Ref`] of type `T` implements [`Debug`]. This is needed to get around a limitation of rust's 
/// type system - currently, there is no way to specify that [`Ref<T>`] must implement [`Debug`] if `T` does,
/// which would make this bound true for any `T: Debug` for any mutability.
/// 
/// [`Ref`]: Mutability::Ref
/// [`Ref<T>`]: Mutability::Ref
pub trait RefDebug<'a, T: Debug + 'a>: Mutability<Ref<'a, T>: Debug> {}

impl<'a, T: Debug + 'a> RefDebug<'a, T> for Immutable {}
impl<'a, T: Debug + 'a> RefDebug<'a, T> for Mutable {}

/// An extension trait to give the [`VirtAddr`] type the [`as_generic_ptr`] method
/// 
/// [`as_generic_ptr`]: VirtAddrGenericMutabilityExt::as_generic_ptr
pub trait VirtAddrGenericMutabilityExt<M: Mutability>: Copy {
    /// Converts the [`VirtAddr`] to a pointer with a generic, possibly inferred mutability.
    /// This is the equivalent of the [`as_ptr`] and [`as_mut_ptr`] methods.
    /// 
    /// [`as_ptr`]: VirtAddr::as_ptr
    /// [`as_mut_ptr`]: VirtAddr::as_mut_ptr
    fn as_generic_ptr<T>(self) -> M::Ptr<T>;
}

impl<M: Mutability> VirtAddrGenericMutabilityExt<M> for VirtAddr {
    fn as_generic_ptr<T>(self) -> M::Ptr<T> {
        M::Ptr::from_usize(self.as_u64().try_into().unwrap())
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
