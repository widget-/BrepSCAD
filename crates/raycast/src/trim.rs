//! Trim regions in surface parameter space.

/// Boundary loops of a trimmed face, given as closed polylines in `(u, v)`.
///
/// All loops are tested together with the even-odd rule, so outer boundaries
/// and holes need no distinction. An empty region means the surface is
/// untrimmed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrimRegion {
    loops: Vec<Vec<[f64; 2]>>,
    /// Period of the `u` parameter for surfaces of revolution; `None` when `u`
    /// is not periodic (planes).
    u_period: Option<f64>,
    /// Period of the `v` parameter for doubly-periodic surfaces such as a
    /// torus; `None` otherwise.
    v_period: Option<f64>,
}

impl TrimRegion {
    pub fn new(loops: Vec<Vec<[f64; 2]>>) -> Self {
        Self {
            loops,
            u_period: None,
            v_period: None,
        }
    }

    /// A region whose `u` parameter wraps, for surfaces of revolution.
    pub fn with_u_period(loops: Vec<Vec<[f64; 2]>>, u_period: f64) -> Self {
        Self {
            loops,
            u_period: Some(u_period),
            v_period: None,
        }
    }

    /// A region whose `u` and `v` parameters both wrap, for a torus.
    pub fn with_periods(
        loops: Vec<Vec<[f64; 2]>>,
        u_period: Option<f64>,
        v_period: Option<f64>,
    ) -> Self {
        Self {
            loops,
            u_period,
            v_period,
        }
    }

    pub fn loops(&self) -> &[Vec<[f64; 2]>] {
        &self.loops
    }

    /// Period of the `u` parameter, when it wraps.
    pub fn u_period(&self) -> Option<f64> {
        self.u_period
    }

    /// Period of the `v` parameter, when it wraps.
    pub fn v_period(&self) -> Option<f64> {
        self.v_period
    }

    pub fn is_empty(&self) -> bool {
        self.loops.is_empty()
    }

    /// Even-odd containment test over every loop.
    pub fn contains(&self, uv: [f64; 2]) -> bool {
        if self.loops.is_empty() {
            return true;
        }
        let mut inside = false;
        for loop_ in &self.loops {
            if loop_.len() < 3 {
                continue;
            }
            let u = match self.u_period {
                Some(period) => unwrap_near(uv[0], loop_, period, 0),
                None => uv[0],
            };
            let v = match self.v_period {
                Some(period) => unwrap_near(uv[1], loop_, period, 1),
                None => uv[1],
            };
            if point_in_polygon(loop_, [u, v]) {
                inside = !inside;
            }
        }
        inside
    }
}

/// Shifts one parameter by whole periods so it is tested near the loop it
/// belongs to.
fn unwrap_near(value: f64, loop_: &[[f64; 2]], period: f64, component: usize) -> f64 {
    if !value.is_finite() || period <= 0.0 {
        return value;
    }
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for point in loop_ {
        min = min.min(point[component]);
        max = max.max(point[component]);
    }
    if !min.is_finite() || !max.is_finite() {
        return value;
    }
    let centre = (min + max) / 2.0;
    let mut best = value;
    let mut best_distance = (value - centre).abs();
    for shift in -2..=2 {
        let candidate = value + shift as f64 * period;
        let distance = (candidate - centre).abs();
        if distance < best_distance {
            best = candidate;
            best_distance = distance;
        }
    }
    best
}

fn point_in_polygon(polygon: &[[f64; 2]], point: [f64; 2]) -> bool {
    let mut inside = false;
    let mut previous = polygon[polygon.len() - 1];
    for current in polygon {
        let crosses = (current[1] > point[1]) != (previous[1] > point[1]);
        if crosses {
            let t = (point[1] - current[1]) / (previous[1] - current[1]);
            let u = current[0] + t * (previous[0] - current[0]);
            if point[0] < u {
                inside = !inside;
            }
        }
        previous = *current;
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    fn square(half: f64) -> Vec<[f64; 2]> {
        vec![[-half, -half], [half, -half], [half, half], [-half, half]]
    }

    #[test]
    fn empty_region_accepts_every_point() {
        let region = TrimRegion::new(Vec::new());
        assert!(region.contains([100.0, -50.0]));
    }

    #[test]
    fn square_boundary_loops_work() {
        let region = TrimRegion::new(vec![square(1.0)]);
        assert!(region.contains([0.0, 0.0]));
        assert!(region.contains([0.9, 0.9]));
        assert!(!region.contains([1.1, 0.0]));
    }

    #[test]
    fn holes_are_excluded_by_the_even_odd_rule() {
        let region = TrimRegion::new(vec![
            square(2.0),
            vec![[-1.0, -1.0], [-1.0, 1.0], [1.0, 1.0], [1.0, -1.0]],
        ]);
        assert!(!region.contains([0.0, 0.0]));
        assert!(region.contains([1.5, 0.0]));
    }

    #[test]
    fn periodic_v_tests_the_nearest_image() {
        // A loop just below the v seam.
        let region = TrimRegion::with_periods(
            vec![vec![
                [0.0, 6.0],
                [1.0, 6.0],
                [1.0, TAU + 0.1],
                [0.0, TAU + 0.1],
            ]],
            None,
            Some(TAU),
        );
        assert!(region.contains([0.5, 6.05]));
        assert!(region.contains([0.5, 0.05]));
        assert!(!region.contains([0.5, 3.0]));
    }

    #[test]
    fn periodic_u_tests_the_nearest_image() {
        // A loop just below the seam: u in [6.0, 2*pi] and [0, 0.1].
        let region = TrimRegion::with_u_period(
            vec![vec![
                [6.0, 0.0],
                [TAU, 0.0],
                [TAU + 0.1, 0.0],
                [TAU + 0.1, 1.0],
                [6.0, 1.0],
            ]],
            TAU,
        );
        // 6.05 is inside; 0.05 is the same point one period away.
        assert!(region.contains([6.05, 0.5]));
        assert!(region.contains([0.05, 0.5]));
        assert!(!region.contains([3.0, 0.5]));
    }
}
