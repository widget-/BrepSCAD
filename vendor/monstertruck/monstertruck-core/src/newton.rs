//! Implementation of Newton method

use std::ops::{Mul, Sub};

use crate::{cgmath64::*, tolerance::*};

/// the value and jacobian corresponding to parameter
#[derive(Clone, Debug)]
pub struct CalcOutput<V, M> {
    /// the value of function
    pub value: V,
    /// the jacobian of function
    pub derivation: M,
}

/// jacobian of function
pub trait Jacobian<V>: Mul<V, Output = V> + Sized {
    #[doc(hidden)]
    fn invert(self) -> Option<Self>;
}

/// Scalar-generic Jacobian for 1D (scalar as its own jacobian).
impl<S> Jacobian<S> for S
where S: Mul<S, Output = S> + Zero + One + Copy + PartialEq + std::ops::Div<Output = S>
{
    #[inline(always)]
    fn invert(self) -> Option<Self> {
        if self == S::zero() {
            None
        } else {
            Some(S::one() / self)
        }
    }
}

/// Scalar-generic matrix Jacobians. Requires `S: BaseFloat` for
/// `SquareMatrix::invert`.
macro_rules! impl_jacobian {
    ($matrix: ident, $vector: ident) => {
        impl<S: cgmath::BaseFloat> Jacobian<cgmath::$vector<S>> for cgmath::$matrix<S> {
            #[inline(always)]
            fn invert(self) -> Option<Self> { SquareMatrix::invert(&self) }
        }
    };
}

impl_jacobian!(Matrix2, Vector2);
impl_jacobian!(Matrix3, Vector3);
impl_jacobian!(Matrix4, Vector4);

/// Solve equation by Newton's method
/// # Examples
/// ```
/// # fn main() -> anyhow::Result<()> {
/// use monstertruck_core::{newton::*, assert_near2};
///
/// let function = |x: f64| CalcOutput {
///     value: x * x - 2.0,
///     derivation: 2.0 * x,
/// };
/// let sqrt2 = solve(function, 1.0, 10).map_err(|e| anyhow::anyhow!("{e}"))?;
/// assert_near2!(sqrt2, f64::sqrt(2.0));
/// # Ok(())
/// # }
/// ```
pub fn solve<V, M>(
    function: impl Fn(V) -> CalcOutput<V, M>,
    mut hint: V,
    trials: usize,
) -> Result<V, NewtonLog<V>>
where
    V: Sub<Output = V> + Copy + Tolerance,
    M: Jacobian<V>,
{
    let mut log = NewtonLog::new(cfg!(debug_assertions), trials);
    for _ in 0..=trials {
        log.push(hint);
        let CalcOutput { value, derivation } = function(hint);
        let Some(inv) = derivation.invert() else {
            log.set_degenerate(true);
            return Err(log);
        };
        let next = hint - inv * value;
        if next.near(&hint) {
            return Ok(next);
        }
        hint = next;
    }
    Err(log)
}

/// Scalar-generic Newton solver.
///
/// Like [`solve`] but uses [`ToleranceV2`](crate::scalar::ToleranceV2) instead
/// of the `f64`-bound [`Tolerance`] trait, allowing convergence checks with any
/// scalar whose epsilon implements [`ToleranceScalar`](crate::scalar::ToleranceScalar).
pub fn solve_v2<V, M>(
    function: impl Fn(V) -> CalcOutput<V, M>,
    mut hint: V,
    trials: usize,
) -> Result<V, NewtonLog<V>>
where
    V: Sub<Output = V> + Copy + crate::scalar::ToleranceV2,
    V::Epsilon: crate::scalar::ToleranceScalar,
    M: Jacobian<V>,
{
    let mut log = NewtonLog::new(cfg!(debug_assertions), trials);
    for _ in 0..=trials {
        log.push(hint);
        let CalcOutput { value, derivation } = function(hint);
        let Some(inv) = derivation.invert() else {
            log.set_degenerate(true);
            return Err(log);
        };
        let next = hint - inv * value;
        if next.near_v2(&hint) {
            return Ok(next);
        }
        hint = next;
    }
    Err(log)
}

mod newtonlog {
    use std::fmt::*;
    /// A structure that stores logs for debugging.
    #[derive(Clone, Debug)]
    pub struct NewtonLog<T> {
        log: Option<Vec<T>>,
        degenerate: bool,
    }

    impl<T> NewtonLog<T> {
        /// constructor
        #[inline(always)]
        pub fn new(activate: bool, trials: usize) -> Self {
            match activate {
                true => NewtonLog {
                    log: Some(Vec::with_capacity(trials)),
                    degenerate: false,
                },
                false => NewtonLog {
                    log: None,
                    degenerate: false,
                },
            }
        }
        /// Returns `true` iff the Newton method terminates due to Jacobian degeneracy.
        #[inline(always)]
        pub fn degenerate(&self) -> bool { self.degenerate }
        #[inline(always)]
        pub(super) fn push(&mut self, log: T) {
            if let Some(vec) = &mut self.log {
                vec.push(log)
            }
        }
        #[inline(always)]
        pub(super) fn set_degenerate(&mut self, degenerate: bool) { self.degenerate = degenerate }
    }

    impl<T: Debug> Display for NewtonLog<T> {
        fn fmt(&self, f: &mut Formatter<'_>) -> Result {
            match self.degenerate {
                true => f.pad("Jacobian is dengenerate. ")?,
                false => f.pad("Newton method is not converges. ")?,
            }
            match &self.log {
                None => f.pad(
                    "If you want to see the Newton log, please re-run it with the debug build.",
                ),
                Some(vec) => {
                    f.pad("Newton Log:\n")?;
                    vec.iter()
                        .try_for_each(|log| f.write_fmt(format_args!("{log:?}\n")))
                }
            }
        }
    }
}
pub use newtonlog::NewtonLog;
