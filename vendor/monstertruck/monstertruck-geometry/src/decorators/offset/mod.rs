use super::*;
use monstertruck_core::cgmath64::control_point::ControlPoint;

impl<C, N> OffsetCurve<C, N> {
    /// Constructs an offset curve from an `entity` and an offset curve.
    #[inline(always)]
    pub const fn new(entity: C, offset: N) -> Self { Self { entity, offset } }
    /// Returns a reference to the entity geometry.
    #[inline(always)]
    pub const fn entity(&self) -> &C { &self.entity }
    /// Returns a reference to the offset geometry.
    #[inline(always)]
    pub const fn offset(&self) -> &N { &self.offset }
}

impl<S, N> OffsetSurface<S, N> {
    /// Constructs an offset surface from an `entity` and an offset surface.
    #[inline(always)]
    pub const fn new(entity: S, offset: N) -> Self { Self { entity, offset } }
    /// Returns a reference to the entity geometry.
    #[inline(always)]
    pub const fn entity(&self) -> &S { &self.entity }
    /// Returns a reference to the offset geometry.
    #[inline(always)]
    pub const fn offset(&self) -> &N { &self.offset }
}

impl<C, N> ParametricCurve for OffsetCurve<C, N>
where
    C: ParametricCurve,
    N: ParametricCurve<Point = C::Vector, Vector = C::Vector>,
    C::Point: ControlPoint<f64, Diff = C::Vector>,
    C::Vector: ControlPoint<f64, Diff = C::Vector>,
{
    type Point = C::Point;
    type Vector = C::Vector;
    #[inline(always)]
    fn evaluate(&self, t: f64) -> Self::Point { self.entity.evaluate(t) + self.offset.evaluate(t) }
    #[inline(always)]
    fn derivative(&self, t: f64) -> Self::Vector {
        self.entity.derivative(t) + self.offset.derivative(t)
    }
    #[inline(always)]
    fn derivative_2(&self, t: f64) -> Self::Vector {
        self.entity.derivative_2(t) + self.offset.derivative_2(t)
    }
    #[inline(always)]
    fn derivative_n(&self, n: usize, t: f64) -> Self::Vector {
        self.entity.derivative_n(n, t) + self.offset.derivative_n(n, t)
    }
    #[inline(always)]
    fn derivatives(&self, max_order: usize, t: f64) -> CurveDerivatives<Self::Vector> {
        self.entity
            .derivatives(max_order, t)
            .element_wise_derivatives(&self.offset.derivatives(max_order, t), |x, y| x + y)
    }
    /// Inherits the parameter range from the entity curve.
    #[inline(always)]
    fn parameter_range(&self) -> ParameterRange { self.entity.parameter_range() }
    #[inline(always)]
    fn period(&self) -> Option<f64> {
        match (self.entity.period(), self.offset.period()) {
            (Some(x), Some(y)) if x.near(&y) => Some((x + y) / 2.0),
            _ => None,
        }
    }
}

impl<S, N> ParametricSurface for OffsetSurface<S, N>
where
    S: ParametricSurface,
    N: ParametricSurface<Point = S::Vector, Vector = S::Vector>,
    S::Point: ControlPoint<f64, Diff = S::Vector>,
    S::Vector: ControlPoint<f64, Diff = S::Vector>,
{
    type Point = S::Point;
    type Vector = S::Vector;
    #[inline(always)]
    fn evaluate(&self, u: f64, v: f64) -> Self::Point {
        self.entity.evaluate(u, v) + self.offset.evaluate(u, v)
    }
    #[inline(always)]
    fn derivative_u(&self, u: f64, v: f64) -> Self::Vector {
        self.entity.derivative_u(u, v) + self.offset.derivative_u(u, v)
    }
    #[inline(always)]
    fn derivative_v(&self, u: f64, v: f64) -> Self::Vector {
        self.entity.derivative_v(u, v) + self.offset.derivative_v(u, v)
    }
    #[inline(always)]
    fn derivative_uu(&self, u: f64, v: f64) -> Self::Vector {
        self.entity.derivative_uu(u, v) + self.offset.derivative_uu(u, v)
    }
    #[inline(always)]
    fn derivative_uv(&self, u: f64, v: f64) -> Self::Vector {
        self.entity.derivative_uv(u, v) + self.offset.derivative_uv(u, v)
    }
    #[inline(always)]
    fn derivative_vv(&self, u: f64, v: f64) -> Self::Vector {
        self.entity.derivative_vv(u, v) + self.offset.derivative_vv(u, v)
    }
    #[inline(always)]
    fn derivative_mn(&self, m: usize, n: usize, u: f64, v: f64) -> Self::Vector {
        self.entity.derivative_mn(m, n, u, v) + self.offset.derivative_mn(m, n, u, v)
    }
    #[inline(always)]
    fn derivatives(&self, max_order: usize, u: f64, v: f64) -> SurfaceDerivatives<Self::Vector> {
        self.entity
            .derivatives(max_order, u, v)
            .element_wise_derivatives(&self.offset.derivatives(max_order, u, v), |x, y| x + y)
    }
    /// Inherits the parameter range from the entity surface.
    #[inline(always)]
    fn parameter_range(&self) -> (ParameterRange, ParameterRange) { self.entity.parameter_range() }
    #[inline(always)]
    fn period_u(&self) -> Option<f64> {
        match (self.entity.period_u(), self.offset.period_u()) {
            (Some(x), Some(y)) if x.near(&y) => Some((x + y) / 2.0),
            _ => None,
        }
    }
    #[inline(always)]
    fn period_v(&self) -> Option<f64> {
        match (self.entity.period_v(), self.offset.period_v()) {
            (Some(x), Some(y)) if x.near(&y) => Some((x + y) / 2.0),
            _ => None,
        }
    }
}

impl<T, F> NormalOffsetField<T, F> {
    /// Constructs a normal offset field from `entity` and a scalar magnitude function.
    #[inline(always)]
    pub fn new(entity: T, scalar: F) -> Self { Self { entity, scalar } }
    /// Returns a reference to the entity geometry.
    #[inline(always)]
    pub const fn entity(&self) -> &T { &self.entity }
    /// Returns a reference to the scalar magnitude function.
    #[inline(always)]
    pub const fn scalar(&self) -> &F { &self.scalar }
}

impl<C, F> ParametricCurve for NormalOffsetField<C, F>
where
    C: ParametricCurve2D,
    F: CurveScalarFunction,
{
    type Point = Vector2;
    type Vector = Vector2;
    #[inline(always)]
    fn derivatives(&self, max_order: usize, t: f64) -> CurveDerivatives<Vector2> {
        // The unit-normal direction in the plane is the 90-degree CCW rotation
        // of the tangent: `(t.x, t.y) -> (t.y, -t.x)`. We start from the first
        // through (max_order + 1)-th derivatives of the entity curve and rotate
        // each in place, which gives derivatives 0..=max_order of the unit normal
        // (after stripping the leading entity-evaluation term via `.derivative()`).
        let mut derivatives = self.entity.derivatives(max_order + 1, t).derivative();
        derivatives
            .iter_mut()
            .for_each(|vec| *vec = Vector2::new(vec.y, -vec.x));
        let scalar_derivatives = self.scalar.derivatives(max_order, t);
        derivatives.combinatorial_derivatives(&scalar_derivatives, |x, y| x * y)
    }
    #[inline(always)]
    fn evaluate(&self, t: f64) -> Self::Point { self.derivatives(0, t)[0] }
    #[inline(always)]
    fn derivative(&self, t: f64) -> Self::Vector { self.derivatives(1, t)[1] }
    #[inline(always)]
    fn derivative_2(&self, t: f64) -> Self::Vector { self.derivatives(2, t)[2] }
    #[inline(always)]
    fn derivative_n(&self, n: usize, t: f64) -> Self::Vector { self.derivatives(n, t)[n] }
    #[inline(always)]
    fn parameter_range(&self) -> ParameterRange { self.entity.parameter_range() }
}

impl<S, F> ParametricSurface for NormalOffsetField<S, F>
where
    S: ParametricSurface3D,
    F: SurfaceScalarFunction,
{
    type Point = Vector3;
    type Vector = Vector3;
    #[inline(always)]
    fn derivatives(&self, max_order: usize, u: f64, v: f64) -> SurfaceDerivatives<Self::Vector> {
        let surface_derivatives = self.entity.derivatives(max_order + 1, u, v);
        let uders = surface_derivatives.derivative_u();
        let vders = surface_derivatives.derivative_v();

        // Cross product of the partial derivatives gives the (un-normalised) surface normal;
        // normalising and combining with the scalar field gives the offset magnitude vector.
        let normal_derivatives = uders.combinatorial_derivatives(&vders, Vector3::cross);
        let normalised_derivatives = normal_derivatives
            .element_wise_derivatives(&normal_derivatives.absolute_derivatives(), Vector3::extend)
            .rational_derivatives();

        normalised_derivatives
            .combinatorial_derivatives(&self.scalar.derivatives(max_order, u, v), |x, y| x * y)
    }
    #[inline(always)]
    fn derivative_mn(&self, m: usize, n: usize, u: f64, v: f64) -> Self::Vector {
        self.derivatives(m + n, u, v)[m][n]
    }
    #[inline(always)]
    fn evaluate(&self, u: f64, v: f64) -> Self::Point { self.derivative_mn(0, 0, u, v) }
    #[inline(always)]
    fn derivative_u(&self, u: f64, v: f64) -> Self::Vector { self.derivative_mn(1, 0, u, v) }
    #[inline(always)]
    fn derivative_v(&self, u: f64, v: f64) -> Self::Vector { self.derivative_mn(0, 1, u, v) }
    #[inline(always)]
    fn derivative_uu(&self, u: f64, v: f64) -> Self::Vector { self.derivative_mn(2, 0, u, v) }
    #[inline(always)]
    fn derivative_uv(&self, u: f64, v: f64) -> Self::Vector { self.derivative_mn(1, 1, u, v) }
    #[inline(always)]
    fn derivative_vv(&self, u: f64, v: f64) -> Self::Vector { self.derivative_mn(0, 2, u, v) }
}
