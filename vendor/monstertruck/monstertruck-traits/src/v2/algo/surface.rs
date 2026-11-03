use crate::v2;
use monstertruck_core::MetricSpace;
use num_traits::Float;

/// Divides the domain into equal parts and returns `(u, v)` such that
/// `surface.evaluate(u, v)` is closest to `point`.
///
/// Scalar-generic version of [`crate::algo::surface::presearch`].
pub fn presearch<S, Surf>(
    surface: &Surf,
    point: Surf::Point,
    (urange, vrange): ((S, S), (S, S)),
    division: usize,
) -> (S, S)
where
    S: Float,
    Surf: v2::ParametricSurface<Scalar = S>,
    Surf::Point: MetricSpace<Metric = S> + Copy,
{
    let mut res = (S::zero(), S::zero());
    let mut min = S::infinity();
    let ((u0, u1), (v0, v1)) = (urange, vrange);
    let n = S::from(division).unwrap();
    for i in 0..=division {
        for j in 0..=division {
            let p = S::from(i).unwrap() / n;
            let q = S::from(j).unwrap() / n;
            let u = u0 * (S::one() - p) + u1 * p;
            let v = v0 * (S::one() - q) + v1 * q;
            let dist = surface.evaluate(u, v).distance2(point);
            if dist < min {
                min = dist;
                res = (u, v);
            }
        }
    }
    res
}
