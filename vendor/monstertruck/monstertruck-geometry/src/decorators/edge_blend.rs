use super::*;

impl<C0, S0, C1, S1, F0, F1> EdgeBlendSurface<C0, S0, F0, C1, S1, F1> {
    /// Constructor
    #[inline]
    pub fn new(
        pcurve0: ParameterCurve<C0, S0>,
        magnitude0: F0,
        pcurve1: ParameterCurve<C1, S1>,
        magnitude1: F1,
    ) -> Self {
        Self {
            pcurve0,
            magnitude0,
            pcurve1,
            magnitude1,
        }
    }
    /// Returns the first boundary parameter curve.
    #[inline]
    pub fn pcurve0(&self) -> &ParameterCurve<C0, S0> { &self.pcurve0 }
    /// Returns the second boundary parameter curve.
    #[inline]
    pub fn pcurve1(&self) -> &ParameterCurve<C1, S1> { &self.pcurve1 }
    /// Returns a mutable reference to the first boundary parameter curve.
    #[inline]
    pub fn pcurve0_mut(&mut self) -> &mut ParameterCurve<C0, S0> { &mut self.pcurve0 }
    /// Returns a mutable reference to the second boundary parameter curve.
    #[inline]
    pub fn pcurve1_mut(&mut self) -> &mut ParameterCurve<C1, S1> { &mut self.pcurve1 }
    /// Returns the first tangent-magnitude function.
    #[inline]
    pub fn magnitude0(&self) -> &F0 { &self.magnitude0 }
    /// Returns the second tangent-magnitude function.
    #[inline]
    pub fn magnitude1(&self) -> &F1 { &self.magnitude1 }
    /// Returns a mutable reference to the first tangent-magnitude function.
    #[inline]
    pub fn magnitude0_mut(&mut self) -> &mut F0 { &mut self.magnitude0 }
    /// Returns a mutable reference to the second tangent-magnitude function.
    #[inline]
    pub fn magnitude1_mut(&mut self) -> &mut F1 { &mut self.magnitude1 }
}

/// The `order`-th derivative (with respect to `u`) of the four cubic Bezier
/// basis functions evaluated at `u`.
const fn bezier_cubic_basis(order: usize, u: f64) -> [f64; 4] {
    let one_minus_u = 1.0 - u;
    match order {
        0 => [
            one_minus_u * one_minus_u * one_minus_u,
            3.0 * one_minus_u * one_minus_u * u,
            3.0 * one_minus_u * u * u,
            u * u * u,
        ],
        1 => [
            -3.0 * one_minus_u * one_minus_u,
            3.0 * one_minus_u * (1.0 - 3.0 * u),
            3.0 * u * (2.0 - 3.0 * u),
            3.0 * u * u,
        ],
        2 => [
            6.0 * one_minus_u,
            -6.0 * (2.0 - 3.0 * u),
            6.0 * (1.0 - 3.0 * u),
            6.0 * u,
        ],
        3 => [-6.0, 18.0, -18.0, 6.0],
        _ => [0.0; 4],
    }
}

// Derivatives of the unit-length field `v / |v|` from the derivatives of `v`,
// obtained by lifting `(v, |v|)` into homogeneous coordinates and projecting.
fn normalized_derivatives(derivatives: &CurveDerivatives<Vector3>) -> CurveDerivatives<Vector3> {
    derivatives
        .element_wise_derivatives(&derivatives.absolute_derivatives(), Vector3::extend)
        .rational_derivatives()
}

// Along the boundary at parameter `u`, returns the derivatives (with respect to
// `u`) of the surface point on the boundary and of the surface normal there.
fn parameter_curve_normal_derivatives<C, S>(
    pcurve: &ParameterCurve<C, S>,
    max_order: usize,
    u: f64,
) -> (CurveDerivatives<Vector3>, CurveDerivatives<Vector3>)
where
    C: ParametricCurve2D,
    S: ParametricSurface3D,
{
    let cders = pcurve.curve().derivatives(max_order + 1, u);
    let Vector2 { x, y } = cders[0];
    let sders = pcurve.surface().derivatives(max_order + 1, x, y);
    let pders = sders.composite_derivatives(&cders);
    let uders = sders.derivative_u().composite_derivatives(&cders);
    let vders = sders.derivative_v().composite_derivatives(&cders);
    let normal_ders = uders.combinatorial_derivatives(&vders, Vector3::cross);
    (pders, normal_ders)
}

// Derivatives (with respect to `u`) of the inner Bezier control-point offset:
// the boundary tangent that is perpendicular to the boundary and lies in the
// surface's tangent plane, scaled by the magnitude function.
fn tangent_derivatives(
    pders: &CurveDerivatives<Vector3>,
    normal_ders: &CurveDerivatives<Vector3>,
    magnitude_derivatives: &CurveDerivatives<f64>,
) -> CurveDerivatives<Vector3> {
    let axis_ders = pders
        .derivative()
        .combinatorial_derivatives(normal_ders, Vector3::cross);
    normalized_derivatives(&axis_ders)
        .combinatorial_derivatives(magnitude_derivatives, |axis, magnitude| axis * magnitude)
}

// Derivatives of the two Bezier control points contributed by one boundary: the
// boundary point itself and the inner control point one-third of the tangent
// away from it.
fn edge_control_point_derivatives<C, S, F>(
    pcurve: &ParameterCurve<C, S>,
    magnitude: &F,
    max_order: usize,
    u: f64,
) -> (CurveDerivatives<Vector3>, CurveDerivatives<Vector3>)
where
    C: ParametricCurve2D,
    S: ParametricSurface3D,
    F: CurveScalarFunction,
{
    let (pders, normal_ders) = parameter_curve_normal_derivatives(pcurve, max_order, u);
    let tangent_ders =
        tangent_derivatives(&pders, &normal_ders, &magnitude.derivatives(max_order, u)) / 3.0;
    (pders, tangent_ders)
}

impl<C0, S0, F0, C1, S1, F1> ParametricSurface for EdgeBlendSurface<C0, S0, F0, C1, S1, F1>
where
    C0: ParametricCurve2D,
    S0: ParametricSurface3D,
    F0: CurveScalarFunction,
    C1: ParametricCurve2D,
    S1: ParametricSurface3D,
    F1: CurveScalarFunction,
{
    type Point = Point3;
    type Vector = Vector3;
    fn derivatives(&self, max_order: usize, u: f64, v: f64) -> SurfaceDerivatives<Self::Vector> {
        let (pders0, tangent_ders0) =
            edge_control_point_derivatives(&self.pcurve0, &self.magnitude0, max_order, u);
        let (pders1, tangent_ders1) =
            edge_control_point_derivatives(&self.pcurve1, &self.magnitude1, max_order, u);
        let mut derivatives = SurfaceDerivatives::new(max_order);
        derivatives
            .slice_iter_mut()
            .enumerate()
            .for_each(|(m, row)| {
                row.iter_mut().enumerate().for_each(|(n, derivative)| {
                    let basis = bezier_cubic_basis(n, v);
                    let q0 = pders0[m];
                    let q1 = pders0[m] + tangent_ders0[m];
                    let q2 = pders1[m] - tangent_ders1[m];
                    let q3 = pders1[m];
                    *derivative = q0 * basis[0] + q1 * basis[1] + q2 * basis[2] + q3 * basis[3];
                });
            });
        derivatives
    }
    #[inline]
    fn derivative_mn(&self, m: usize, n: usize, u: f64, v: f64) -> Self::Vector {
        self.derivatives(m + n, u, v)[m][n]
    }
    #[inline]
    fn evaluate(&self, u: f64, v: f64) -> Self::Point {
        Point3::from_vec(self.derivatives(0, u, v)[0][0])
    }
    #[inline]
    fn derivative_u(&self, u: f64, v: f64) -> Self::Vector { self.derivative_mn(1, 0, u, v) }
    #[inline]
    fn derivative_v(&self, u: f64, v: f64) -> Self::Vector { self.derivative_mn(0, 1, u, v) }
    #[inline]
    fn derivative_uu(&self, u: f64, v: f64) -> Self::Vector { self.derivative_mn(2, 0, u, v) }
    #[inline]
    fn derivative_uv(&self, u: f64, v: f64) -> Self::Vector { self.derivative_mn(1, 1, u, v) }
    #[inline]
    fn derivative_vv(&self, u: f64, v: f64) -> Self::Vector { self.derivative_mn(0, 2, u, v) }
    #[inline]
    fn parameter_range(&self) -> (ParameterRange, ParameterRange) {
        let range0 = self.pcurve0.parameter_range();
        let range1 = self.pcurve1.parameter_range();
        let range = range_common_part(&range0, &range1);
        (range, (Bound::Included(0.0), Bound::Included(1.0)))
    }
}

impl<C0, S0, F0, C1, S1, F1> ParametricSurface3D for EdgeBlendSurface<C0, S0, F0, C1, S1, F1>
where
    C0: ParametricCurve2D,
    S0: ParametricSurface3D,
    F0: CurveScalarFunction,
    C1: ParametricCurve2D,
    S1: ParametricSurface3D,
    F1: CurveScalarFunction,
{
}

impl<C0, S0, F0, C1, S1, F1> BoundedSurface for EdgeBlendSurface<C0, S0, F0, C1, S1, F1>
where
    C0: BoundedCurve + ParametricCurve2D,
    S0: ParametricSurface3D,
    F0: CurveScalarFunction,
    C1: BoundedCurve + ParametricCurve2D,
    S1: ParametricSurface3D,
    F1: CurveScalarFunction,
{
}

impl<C0, S0, F0, C1, S1, F1> ParameterDivision2D for EdgeBlendSurface<C0, S0, F0, C1, S1, F1>
where
    C0: ParametricCurve2D,
    S0: ParametricSurface3D,
    F0: CurveScalarFunction,
    C1: ParametricCurve2D,
    S1: ParametricSurface3D,
    F1: CurveScalarFunction,
{
    fn parameter_division(
        &self,
        range: ((f64, f64), (f64, f64)),
        tol: f64,
    ) -> (Vec<f64>, Vec<f64>) {
        algo::surface::parameter_division(self, range, tol)
    }
}

impl<C0, S0, F0, C1, S1, F1> SearchNearestParameter<SurfaceParameter>
    for EdgeBlendSurface<C0, S0, F0, C1, S1, F1>
where
    C0: BoundedCurve + ParametricCurve2D,
    S0: ParametricSurface3D,
    F0: CurveScalarFunction,
    C1: BoundedCurve + ParametricCurve2D,
    S1: ParametricSurface3D,
    F1: CurveScalarFunction,
{
    type Point = Point3;
    fn search_nearest_parameter<H: Into<SearchParameterHint2D>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<(f64, f64)> {
        let hint = match hint.into() {
            SearchParameterHint2D::Parameter(x, y) => (x, y),
            SearchParameterHint2D::Range(range0, range1) => {
                algo::surface::presearch(self, point, (range0, range1), PRESEARCH_DIVISION)
            }
            SearchParameterHint2D::None => {
                algo::surface::presearch(self, point, self.range_tuple(), PRESEARCH_DIVISION)
            }
        };
        algo::surface::search_nearest_parameter(self, point, hint, trials)
    }
}

impl<C0, S0, F0, C1, S1, F1> SearchParameter<SurfaceParameter>
    for EdgeBlendSurface<C0, S0, F0, C1, S1, F1>
where
    C0: BoundedCurve + ParametricCurve2D,
    S0: ParametricSurface3D,
    F0: CurveScalarFunction,
    C1: BoundedCurve + ParametricCurve2D,
    S1: ParametricSurface3D,
    F1: CurveScalarFunction,
{
    type Point = Point3;
    fn search_parameter<H: Into<SearchParameterHint2D>>(
        &self,
        point: Self::Point,
        hint: H,
        trials: usize,
    ) -> Option<(f64, f64)> {
        let hint = match hint.into() {
            SearchParameterHint2D::Parameter(x, y) => (x, y),
            SearchParameterHint2D::Range(range0, range1) => {
                algo::surface::presearch(self, point, (range0, range1), PRESEARCH_DIVISION)
            }
            SearchParameterHint2D::None => {
                algo::surface::presearch(self, point, self.range_tuple(), PRESEARCH_DIVISION)
            }
        };
        algo::surface::search_parameter(self, point, hint, trials)
    }
}
