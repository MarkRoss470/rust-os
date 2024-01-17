//! Contains the [`IteratorListDebug`] struct for printing out iterators

use core::{cell::RefCell, fmt::Debug};

/// What formatter the [`Debug`] impl of [`IteratorListDebug`] will pass to each element
enum Format {
    /// Pass on the formatter given
    PassThrough,
    /// Always use the default [`Debug`] formatter
    DefaultDebug,
    /// Always use the alternate (pretty-print) [`Debug`] formatter
    AlternateDebug,
}

/// A utility struct for implementing [`Debug`] for an iterator such that it prints out all the values in a list format,
/// without allocating.
///
/// ```
/// let iter = 0..10;
/// println!("{:?}", iter); // Prints "0..10"
/// println!("{:?}", IteratorListDebug::new(iter)); // Prints "[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]"
///
/// ```
///
/// If constructed with the [`new`] method, the formatter given to the [`Debug`]
/// implementation will be passed on to each element, however if [`new_with_default_formatting`]
/// or [`new_with_alternate_formatting`] are used then a new formatter is created with the given attributes.
/// This can be used to make the output more compact, such as forcing the elements to print on one line each.
///
/// [`new`]: [IteratorListDebug::new]
/// [`new_with_default_formatting`]: [IteratorListDebug::new_with_default_formatting]
/// [`new_with_alternate_formatting`]: [IteratorListDebug::new_with_alternate_formatting]
///
pub struct IteratorListDebug<T, U>
where
    T: Debug,
    U: Iterator<Item = T>,
{
    /// The iterator, held in a [`RefCell`] so that the iterator can be iterated (requiring a mutable reference)
    /// inside the impl of [`Debug`], which gives a shared reference.
    iterator: RefCell<U>,
    /// The [`Format`] which will be applied to the arguments
    format: Format,
}

impl<T, U> IteratorListDebug<T, U>
where
    T: Debug,
    U: Iterator<Item = T>,
{
    /// Wraps the iterator in an [`IteratorListDebug`].
    /// The implementation of [`Debug`] will pass through formatting arguments.
    pub fn new(iterator: U) -> Self {
        Self {
            iterator: RefCell::new(iterator),
            format: Format::PassThrough,
        }
    }

    /// Wraps the iterator in an [`IteratorListDebug`].
    /// The implementation of [`Debug`] will always use the default debug format when debugging an item
    /// (i.e. `format_args!("{item:?}")`).
    pub fn new_with_default_formatting(iterator: U) -> Self {
        Self {
            iterator: RefCell::new(iterator),
            format: Format::DefaultDebug,
        }
    }

    /// Wraps the iterator in an [`IteratorListDebug`].
    /// The implementation of [`Debug`] will always use the alternate debug format when debugging an item.
    /// (i.e. `format_args!("{item:#?}")`).
    /// For types whose [`Debug`] impls are macro derived, this usually means pretty printing the argument.
    pub fn new_with_alternate_formatting(iterator: U) -> Self {
        Self {
            iterator: RefCell::new(iterator),
            format: Format::AlternateDebug,
        }
    }
}

impl<T, U> Debug for IteratorListDebug<T, U>
where
    T: Debug,
    U: Iterator<Item = T>,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let mut l = f.debug_list();
        let iterator = &mut *self.iterator.borrow_mut();

        for entry in iterator {
            match self.format {
                Format::PassThrough => l.entry(&entry),
                Format::DefaultDebug => l.entry(&format_args!("{entry:?}")),
                Format::AlternateDebug => l.entry(&format_args!("{entry:#?}")),
            };
        }

        l.finish()
    }
}
