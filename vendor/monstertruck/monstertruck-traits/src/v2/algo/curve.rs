use crate::v2;
use monstertruck_core::MetricSpace;
use num_traits::Float;

/// Divides the domain into equal parts and returns `t` such that
/// `curve.evaluate(t)` is closest to `point`.
///
/// Scalar-generic version of [`crate::algo::curve::presearch`].
pub fn presearch<S, C>(curve: &C, point: C::Point, range: (S, S), division: usize) -> S
where
    S: Float,
    C: v2::ParametricCurve<Scalar = S>,
    C::Point: MetricSpace<Metric = S> + Copy, {
    let (t0, t1) = range;
    let mut res = t0;
    let mut min = S::infinity();
    let n = S::from(division).unwrap();
    for i in 0..=division {
        let p = S::from(i).unwrap() / n;
        let t = t0 * (S::one() - p) + t1 * p;
        let dist = curve.evaluate(t).distance2(point);
        if dist < min {
            min = dist;
            res = t;
        }
    }
    res
}
