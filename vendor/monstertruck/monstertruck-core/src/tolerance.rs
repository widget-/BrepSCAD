use crate::cgmath64::*;
use cgmath::AbsDiffEq;
use std::fmt::Debug;

/// general tolerance
pub const TOLERANCE: f64 = 1.0e-6;

/// general tolerance of square order
pub const TOLERANCE2: f64 = TOLERANCE * TOLERANCE;

/// Role-separated tolerances, after OCCT's `Precision` package.
///
/// The kernel threads one `tol` through every decision, which conflates four
/// different questions: whether two points are the same point, whether two
/// directions are parallel, whether two objects meet at all, and how finely to
/// sample a curve. OCCT keeps these apart - `Confusion()` 1e-7, `Angular()`
/// 1e-12, `Intersection()` Confusion/100, `Approximation()` Confusion*10 - and
/// this type does the same, plus the caller's modelling tolerance, which OCCT
/// carries per shape rather than as a `Precision` constant.
///
/// Use [`approximation`](Precision::approximation) for sampling fidelity and
/// [`confusion`](Precision::confusion) for coincidence. They differ by five
/// orders of magnitude, and a decision that mixes them will fail near tangency.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Precision {
    /// The caller's tolerance: how far apart two *shapes* may be and still be
    /// considered coincident. OCCT keeps this per shape; here it is the value
    /// the boolean pipeline is invoked with.
    pub modelling: f64,
    /// Below this, two points are the same point.
    pub confusion: f64,
    /// Below this angle, in radians, two directions are parallel or
    /// perpendicular.
    pub angular: f64,
    /// Below this, two objects are considered to meet. Deliberately tighter
    /// than `confusion`, so a near-tangency keeps iterating rather than being
    /// declared a hit and dropped.
    pub intersection: f64,
    /// Curve and surface sampling fidelity. Deliberately looser than
    /// `confusion`, because it trades accuracy for time.
    pub approximation: f64,
}

impl Precision {
    /// The package's general tolerance as `confusion`, with the other roles
    /// derived the way OCCT derives its own.
    pub fn from_modelling(modelling: f64) -> Precision {
        Precision {
            modelling,
            confusion: TOLERANCE,
            // Looser than OCCT's 1e-12: two faces that are meant to coincide
            // are computed independently, so their normals agree only to
            // accumulated rounding.
            angular: 1.0e-7,
            intersection: TOLERANCE * 1.0e-2,
            approximation: modelling,
        }
    }

    /// `confusion` squared, for comparing squared distances without a square
    /// root.
    pub fn square_confusion(&self) -> f64 { self.confusion * self.confusion }
}

/// Defines a tolerance in the whole package
pub trait Tolerance: AbsDiffEq<Epsilon = f64> + Debug {
    /// The "distance" is less than `TOLERANCE`.
    fn near(&self, other: &Self) -> bool { self.abs_diff_eq(other, TOLERANCE) }

    /// The "distance" is less than `TOLERANCR2`.
    fn near2(&self, other: &Self) -> bool { self.abs_diff_eq(other, TOLERANCE2) }
}

impl<T: AbsDiffEq<Epsilon = f64> + Debug> Tolerance for T {}

/// Asserts that `left.near(&right)` (using `Tolerance`).
#[macro_export]
macro_rules! assert_near {
    ($left: expr, $right: expr $(,)?) => {{
        let (left, right) = ($left, $right);
        assert!(
            $crate::tolerance::Tolerance::near(&left, &right),
            "assertion failed: `left` is near `right`\nleft: {left:?},\nright: {right:?}",
        )
    }};
    ($left: expr, $right: expr, $($arg: tt)+) => {{
        let (left, right) = ($left, $right);
        assert!(
            $crate::tolerance::Tolerance::near(&left, &right),
            "assertion failed: `left` is near `right`\nleft: {left:?},\nright: {right:?}: {}",
            format_args!($($arg)+),
        )
    }};
}

/// Similar to `assert_near!`, but returns a test failure instead of panicking if the condition fails.
#[macro_export]
macro_rules! prop_assert_near {
    ($left: expr, $right: expr $(,)?) => {{
        let (left, right) = ($left, $right);
        prop_assert!(
            $crate::tolerance::Tolerance::near(&left, &right),
            "assertion failed: `left` is near `right`\nleft: {left:?},\nright: {right:?}",
        )
    }};
    ($left: expr, $right: expr, $($arg: tt)+) => {{
        let (left, right) = ($left, $right);
        prop_assert!(
            $crate::tolerance::Tolerance::near(&left, &right),
            "assertion failed: `left` is near `right`\nleft: {left:?}, right: {right:?}: {}",
            format_args!($($arg)+),
        )
    }};
}

#[test]
#[should_panic]
fn assert_near_without_msg() { assert_near!(1.0, 2.0) }

#[test]
#[should_panic]
fn assert_near_with_msg() { assert_near!(1.0, 2.0, "{}", "test OK") }

/// Asserts that `left.near2(&right)` (using `Tolerance`).
#[macro_export]
macro_rules! assert_near2 {
    ($left: expr, $right: expr $(,)?) => {{
        let (left, right) = ($left, $right);
        assert!(
            $crate::tolerance::Tolerance::near2(&left, &right),
            "assertion failed: `left` is near `right`\nleft: {left:?},\nright: {right:?}",
        )
    }};
    ($left: expr, $right: expr, $($arg: tt)+) => {{
        let (left, right) = ($left, $right);
        assert!(
            $crate::tolerance::Tolerance::near2(&left, &right),
            "assertion failed: `left` is near `right`\nleft: {left:?},\nright: {right:?}: {}",
            format_args!($($arg)+),
        )
    }};
}

/// Similar to `assert_near2!`, but returns a test failure instead of panicking if the condition fails.
#[macro_export]
macro_rules! prop_assert_near2 {
    ($left: expr, $right: expr $(,)?) => {{
        let (left, right) = ($left, $right);
        prop_assert!(
            $crate::tolerance::Tolerance::near2(&left, &right),
            "assertion failed: `left` is near `right`\nleft: {left:?},\nright: {right:?}",
        )
    }};
    ($left: expr, $right: expr, $($arg: tt)+) => {
        let (left, right) = ($left, $right);
        prop_assert!(
            $crate::tolerance::Tolerance::near2(&left, &right),
            "assertion failed: `left` is near `right`\nleft: {left:?},\nright: {right:?}: {}",
            format_args!($($arg)+),
        )
    };
}

#[test]
#[should_panic]
fn assert_near2_without_msg() { assert_near2!(1.0, 2.0) }

#[test]
#[should_panic]
fn assert_near2_with_msg() { assert_near2!(1.0, 2.0, "{}", "test OK") }

/// The structs defined the origin. `f64`, `Vector`, and so on.
pub trait Origin: Tolerance + Zero {
    /// near origin
    #[inline(always)]
    fn so_small(&self) -> bool { self.near(&Self::zero()) }

    /// near origin in square order
    #[inline(always)]
    fn so_small2(&self) -> bool { self.near2(&Self::zero()) }
}

impl<T: Tolerance + Zero> Origin for T {}
