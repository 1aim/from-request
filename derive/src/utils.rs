use std::hash::{Hash, Hasher};

/// Stores an object of type `T` and implements traits by calling a function
/// returning a proxy `H`.
pub struct ByProxy<T, H: ?Sized> {
    object: T,
    tf: for<'a> fn(&'a T) -> &'a H,
}

impl<T, H: Hash + ?Sized> ByProxy<T, H> {
    /// Creates a new `HashWith` adapter storing `object`.
    ///
    /// The returned `Self` will implement `Hash` by calling `to_hash` and
    /// hashing the returned object.
    pub fn new(object: T, to_hash: for<'a> fn(&'a T) -> &'a H) -> Self {
        Self {
            object,
            tf: to_hash,
        }
    }
}

impl<T, H: Hash + ?Sized> AsRef<T> for ByProxy<T, H> {
    fn as_ref(&self) -> &T {
        &self.object
    }
}

impl<T, H: Hash + ?Sized> Hash for ByProxy<T, H> {
    fn hash<I>(&self, state: &mut I)
    where
        I: Hasher,
    {
        let hashable = (self.tf)(&self.object);
        hashable.hash(state)
    }
}

impl<T, H: PartialEq + ?Sized> PartialEq for ByProxy<T, H> {
    fn eq(&self, other: &Self) -> bool {
        let ours = (self.tf)(&self.object);
        let theirs = (other.tf)(&other.object);

        ours.eq(theirs)
    }
}

impl<T, H: Eq + ?Sized> Eq for ByProxy<T, H> {}
