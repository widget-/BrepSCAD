# `monstertruck-traits`

<!-- cargo-rdme start -->

Geometric trait definitions: `ParametricCurve`, `ParametricSurface`, `BoundedCurve`, `Invertible`, `Transformed`, and more.

## Examples

```rust
use monstertruck_traits::*;
use monstertruck_core::cgmath64::*;

// `range_tuple` comes from `BoundedCurve`, so the bound needs both traits.
fn arc_length<C: ParametricCurve<Point = Point3> + BoundedCurve>(
    curve: &C,
    steps: usize,
) -> f64 {
    let (t0, t1) = curve.range_tuple();
    let dt = (t1 - t0) / steps as f64;
    (0..steps)
        .map(|i| {
            let a = curve.evaluate(t0 + dt * i as f64);
            let b = curve.evaluate(t0 + dt * (i + 1) as f64);
            (b - a).magnitude()
        })
        .sum()
}
```

## Continuity foundations

`ContinuityOrder` provides checked `G0`--`G4` requests, `BoundarySide`
names full tensor-product patch sides, and `SurfaceContinuityCapability`
carries a typed representation-specific support determination without
embedding representation rules in this crate. Unsupported reports preserve
an actionable reason and any known maximum supported order. The report does
not establish two-surface or solver feasibility. `G4` is explicitly
experimental.

```rust
use monstertruck_traits::{
    BoundarySide, ContinuityOrder, SurfaceContinuityCapability,
    UnsupportedContinuityCapability,
};

let order = ContinuityOrder::new(3)?;
let capability = SurfaceContinuityCapability::try_unsupported(
    BoundarySide::MinU,
    order,
    UnsupportedContinuityCapability::InsufficientDegree {
        available: 2,
        required: 3,
    },
    Some(ContinuityOrder::G2),
)?;

assert_eq!(capability.side(), BoundarySide::MinU);
assert_eq!(capability.maximum_supported_order(), Some(ContinuityOrder::G2));
assert!(matches!(
    capability.unsupported_reason(),
    Some(UnsupportedContinuityCapability::InsufficientDegree { .. })
));
assert!(ContinuityOrder::new(5).is_err());
assert!(ContinuityOrder::G4.is_experimental());
```

<!-- cargo-rdme end -->

> Forked from [`truck-geotrait`](https://crates.io/crates/truck-geotrait) v0.4.0 by [ricosjp](https://github.com/ricosjp/truck).

## License

Apache License 2.0
