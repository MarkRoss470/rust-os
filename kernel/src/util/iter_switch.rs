//! Contains the [`IterSwitch`] enum

/// A two-variant enum where each variant is a different [`Iterator`] of type `T`.
/// 
/// This enum is also an [`Iterator`] of type `T`, which delegates its `impl` to whichever variant it contains.
/// This is useful when a function returns an opaque iterator which could be backed by one of two other sources.
/// 
/// ```compile_fail
/// fn iter_1() -> impl Iterator<Item = u8> {
///     0..10
/// }
/// fn iter_2() -> impl Iterator<Item = u8> {
///     100..200
/// }
/// 
/// fn iter_either_1_or_2() -> impl Iterator<Item = u8> {
///     let some_condition = true;
///     if some_condition {
///         iter_1()
///     } else {
///         iter_2()
///     }
/// }
/// ```
/// 
/// This code fails to compile because the return types of `iter_1` and `iter_2` 
/// are different despite having the same written type signature.
/// To fix this, Wrap `iter_1` in [`IterSwitch::A`] and `iter_2` in [`IterSwitch::B`],
/// and rust's type inference will infer the concrete type of `iter_either_1_or_2` 
/// to be [`IterSwitch`].
/// 
/// ```
/// # fn iter_1() -> impl Iterator<Item = u8> {
/// #     0..10
/// # }
/// # fn iter_2() -> impl Iterator<Item = u8> {
/// #     100..200
/// # }
/// # 
/// fn iter_either_1_or_2() -> impl Iterator<Item = u8> {
///     let some_condition = true;
///     if some_condition {
///         IterSwitch::A(iter_1())
///     } else {
///         IterSwitch::B(iter_2())
///     }
/// }
/// ```
/// 
/// [`IterSwitch`]es can also be composed to allow for more than 2 types.
/// 
/// ```
/// fn iter_1() -> impl Iterator<Item = u8> {
///     0..10
/// }
/// fn iter_2() -> impl Iterator<Item = u8> {
///     100..200
/// }
/// fn iter_3() -> impl Iterator<Item = u8> {
///     0..=255
/// }
/// 
/// fn iter_either_1_or_2_or_3() -> impl Iterator<Item = u8> {
///     let some_condition = true;
///     let some_other_condition = true;
///     if some_condition {
///         IterSwitch::A(IterSwitch::A(iter_1()))
///     } else if some_other_condition {
///         IterSwitch::A(IterSwitch::B(iter_2()))
///     } else {
///         IterSwitch::B(iter_3())
///     }
/// }
/// ```
/// 
#[derive(Debug)]
pub enum IterSwitch<T, A, B>
where
    A: Iterator<Item = T>,
    B: Iterator<Item = T>,
{
    /// The first option for the iterator
    A(A),
    /// The second option for the iterator
    B(B),
}

impl<T, A, B> Iterator for IterSwitch<T, A, B>
where
    A: Iterator<Item = T>,
    B: Iterator<Item = T>,
{
    type Item = T;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            IterSwitch::A(ref mut a) => a.next(),
            IterSwitch::B(ref mut b) => b.next(),
        }
    }
}
