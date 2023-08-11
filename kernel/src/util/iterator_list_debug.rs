//! Contains the [`IteratorListDebug`] struct for printing out iterators

use core::{fmt::Debug, cell::RefCell};

/// A utility struct for implementing [`Debug`] for an iterator such that it prints out all the values in a list format,
/// without allocating.
/// 
/// ```
/// let iter = 0..10;
/// println!("{:?}", iter); // Prints "0..10"
/// println!("{:?}", IteratorListDebug::new(iter)); // Prints "[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]"
/// 
/// ```
pub struct IteratorListDebug<T, U>
where
    T: Debug,
    U: Iterator<Item = T>,
{
    /// The iterator, held in a [`RefCell`] so that the iterator can be iterated (requiring a mutable reference)
    /// inside the impl of [`Debug`], which gives a shared reference.
    iterator: RefCell<U>,
}

impl<T, U> IteratorListDebug<T, U>
where
    T: Debug,
    U: Iterator<Item = T>,
{
    pub fn new(iterator: U) -> Self {
        Self { iterator: RefCell::new(iterator) }
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

        l.entries(iterator);

        l.finish()
    }
}
