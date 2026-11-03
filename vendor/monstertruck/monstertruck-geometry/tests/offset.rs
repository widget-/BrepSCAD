use monstertruck_geometry::prelude::*;

#[derive(Clone)]
struct CurveLength;

impl CurveScalarFunction for CurveLength {
    fn derivative_n(&self, n: usize, t: f64) -> f64 {
        match n {
            0 => 1.0 + t + t * t,
            1 => 1.0 + 2.0 * t,
            2 => 2.0,
            _ => 0.0,
        }
    }
}

#[test]
fn normal_offset_field_line_with_variable_length() {
    let line = Line(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0));
    let offset = OffsetCurve::new(line, NormalOffsetField::new(line, CurveLength));
    let t = 0.3;
    let length = 1.0 + t + t * t;

    assert_near!(offset.evaluate(t), Point2::new(t, -length));
    assert_near!(offset.derivative(t), Vector2::new(1.0, -1.0 - 2.0 * t));
    assert_near!(offset.derivative_2(t), Vector2::new(0.0, -2.0));

    let derivatives = offset.derivatives(3, t);
    assert_near!(derivatives[0], Point2::new(t, -length).to_vec());
    assert_near!(derivatives[1], Vector2::new(1.0, -1.0 - 2.0 * t));
    assert_near!(derivatives[2], Vector2::new(0.0, -2.0));
    assert_near!(derivatives[3], Vector2::zero());
}

#[test]
fn normal_offset_field_unit_circle_with_fixed_length() {
    let circle = UnitCircle::<Point2>::new();
    let offset = OffsetCurve::new(circle, NormalOffsetField::new(circle, 1.0));

    for i in 0..=8 {
        let t = i as f64 / 8.0 * std::f64::consts::TAU;
        assert_near!(offset.evaluate(t), circle.evaluate(t) * 2.0);
        assert_near!(offset.derivative(t), circle.derivative(t) * 2.0);
        assert_near!(offset.derivative_2(t), circle.derivative_2(t) * 2.0);
        assert_near!(offset.derivative_n(3, t), circle.derivative_n(3, t) * 2.0);
    }
}

#[derive(Clone)]
struct PlaneLength;

impl SurfaceScalarFunction for PlaneLength {
    fn derivative_mn(&self, m: usize, n: usize, u: f64, v: f64) -> f64 {
        match (m, n) {
            (0, 0) => u * u + u * v + v * v,
            (1, 0) => 2.0 * u + v,
            (0, 1) => u + 2.0 * v,
            (2, 0) | (1, 1) | (0, 2) => 2.0,
            _ => 0.0,
        }
    }
}

#[test]
fn normal_offset_field_surface_derivatives_on_plane() {
    let field = NormalOffsetField::new(Plane::xy(), PlaneLength);
    let (u, v) = (0.2, 0.3);
    let derivatives = field.derivatives(2, u, v);

    assert_near!(
        derivatives[0][0],
        Vector3::new(0.0, 0.0, u * u + u * v + v * v),
    );
    assert_near!(derivatives[1][0], Vector3::new(0.0, 0.0, 2.0 * u + v));
    assert_near!(derivatives[0][1], Vector3::new(0.0, 0.0, u + 2.0 * v));
    assert_near!(derivatives[2][0], Vector3::new(0.0, 0.0, 2.0));
    assert_near!(derivatives[1][1], Vector3::new(0.0, 0.0, 2.0));
    assert_near!(derivatives[0][2], Vector3::new(0.0, 0.0, 2.0));
}
