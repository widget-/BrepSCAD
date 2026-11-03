//! Compatibility adapters between the legacy `f64` traits and the v2
//! scalar-generic traits.
//!
//! Phase 0 used blanket impls (`impl<T: old::Trait> v2::Trait for T`) so
//! existing geometry types automatically gained v2 impls. Those blanket impls
//! were removed at the start of Phase 2 because they conflict with native v2
//! impls on concrete types (Rust coherence rules). Geometry types now implement
//! v2 traits directly in `monstertruck-geometry`.
