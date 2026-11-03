use std::fmt::Debug;
use std::hash::{Hash, Hasher};
use std::marker::PhantomData;

/// Id structure with `Copy`, `Hash` and `Eq` using raw pointers
pub struct Id<T>(usize, PhantomData<T>);

impl<T> Id<T> {
    /// Creates the Id by a raw pointer.
    #[inline(always)]
    pub fn new(ptr: *const T) -> Id<T> { Id(ptr as usize, PhantomData) }
}

impl<T> Clone for Id<T> {
    #[inline(always)]
    fn clone(&self) -> Id<T> { *self }
}

impl<T> Copy for Id<T> {}

impl<T> Hash for Id<T> {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) { self.0.hash(state) }
}

impl<T> PartialEq for Id<T> {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool { self.0 == other.0 }
}

impl<T> Eq for Id<T> {}

/// Renamed to [`Id`] per RFC 430 (C-CASE).
#[deprecated(note = "renamed to Id per RFC 430 (C-CASE)")]
pub type ID<T> = Id<T>;

impl<T> Debug for Id<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        f.write_fmt(format_args!("0x{:x}", self.0))
    }
}

/// Persistent topology element identifier.
///
/// Unlike [`Id`] (derived from Arc pointer addresses, changes on
/// deserialization), `StableId` survives serialization round-trips.
/// Values are unique within a Solid but not globally unique.
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
#[cfg_attr(
    feature = "rkyv",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
#[cfg_attr(feature = "rkyv", rkyv(derive(PartialEq, Eq, PartialOrd, Ord, Hash)))]
pub struct StableId(u64);

impl StableId {
    /// Reserved sentinel for unassigned IDs.
    pub const UNASSIGNED: StableId = StableId(0);

    /// Create a new StableId.
    pub fn new(val: u64) -> Self {
        debug_assert!(val != 0, "StableId 0 is reserved for UNASSIGNED");
        StableId(val)
    }

    /// Get the raw u64 value.
    pub fn raw(self) -> u64 { self.0 }

    /// Whether this ID has been assigned.
    pub fn is_assigned(self) -> bool { self.0 != 0 }
}

impl Default for StableId {
    fn default() -> Self { Self::UNASSIGNED }
}

impl std::fmt::Display for StableId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { write!(f, "#{}", self.0) }
}

/// Monotonic counter for assigning fresh [`StableId`]s within a Solid.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(
    feature = "rkyv",
    derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)
)]
pub struct StableIdAllocator {
    next: u64,
}

impl StableIdAllocator {
    /// Create a new allocator starting at 1.
    pub const fn new() -> Self { Self { next: 1 } }

    /// Allocate the next [`StableId`].
    pub fn allocate(&mut self) -> StableId {
        let id = StableId::new(self.next);
        self.next += 1;
        id
    }

    /// Advance the counter past `ceil` to avoid collisions.
    pub fn ensure_above(&mut self, ceil: u64) {
        if self.next <= ceil {
            self.next = ceil + 1;
        }
    }

    /// Current counter value (next ID that will be allocated).
    pub fn peek(&self) -> u64 { self.next }
}

impl Default for StableIdAllocator {
    fn default() -> Self { Self::new() }
}

#[test]
fn debug_backward_compatibility() {
    let x: f64 = 3.0;
    let id = Id::new(&x);
    let a = format!("{id:?}");
    let b = format!("{:p}", &x);
    assert_eq!(a, b);
}

#[test]
fn stable_id_basics() {
    let id = StableId::new(42);
    assert_eq!(id.raw(), 42);
    assert!(id.is_assigned());
    assert!(!StableId::UNASSIGNED.is_assigned());
}

#[test]
fn stable_id_allocator() {
    let mut alloc = StableIdAllocator::new();
    let a = alloc.allocate();
    let b = alloc.allocate();
    assert_ne!(a, b);
    assert_eq!(a.raw(), 1);
    assert_eq!(b.raw(), 2);
}
