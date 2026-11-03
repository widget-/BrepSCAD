/// Scalar-generic curve-cutting operation.
///
/// Splits a bounded curve at parameter `t`, mutating `self` to be the
/// left portion `[a, t]` and returning the right portion `[t, b]`.
pub trait Cut: super::BoundedCurve {
    /// Cuts the curve at parameter `t`.
    ///
    /// After the call, `self` covers `[a, t]` and the returned value
    /// covers `[t, b]`, where `[a, b]` was the original parameter range.
    fn cut(&mut self, t: Self::Scalar) -> Self;
}
