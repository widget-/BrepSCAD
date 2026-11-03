//! Phase 4.0 benchmark: f32 vs f64 Line evaluation through v2 traits.

use criterion::{Criterion, criterion_group, criterion_main};
use monstertruck_geometry::prelude::*;
use monstertruck_traits::v2;
use std::hint::black_box;

type Point3F32 = cgmath::Point3<f32>;

fn bench_evaluate(c: &mut Criterion) {
    let line_f32: Line<Point3F32> =
        Line(Point3F32::new(1.0, 2.0, 3.0), Point3F32::new(4.0, 5.0, 6.0));
    let line_f64: Line<Point3> = Line(Point3::new(1.0, 2.0, 3.0), Point3::new(4.0, 5.0, 6.0));

    let mut group = c.benchmark_group("line_evaluate");
    group.bench_function("f32", |b| {
        b.iter(|| {
            let mut sum = Point3F32::new(0.0, 0.0, 0.0);
            for i in 0..1000 {
                let t = i as f32 / 999.0;
                let p = v2::ParametricCurve::evaluate(&line_f32, black_box(t));
                let d = v2::ParametricCurve::derivative(&line_f32, black_box(t));
                sum.x += p.x + d.x;
                sum.y += p.y + d.y;
                sum.z += p.z + d.z;
            }
            sum
        })
    });
    group.bench_function("f64", |b| {
        b.iter(|| {
            let mut sum = Point3::new(0.0, 0.0, 0.0);
            for i in 0..1000 {
                let t = i as f64 / 999.0;
                let p = v2::ParametricCurve::evaluate(&line_f64, black_box(t));
                let d = v2::ParametricCurve::derivative(&line_f64, black_box(t));
                sum.x += p.x + d.x;
                sum.y += p.y + d.y;
                sum.z += p.z + d.z;
            }
            sum
        })
    });
    group.finish();
}

fn bench_presearch(c: &mut Criterion) {
    let line_f32: Line<Point3F32> =
        Line(Point3F32::new(0.0, 0.0, 0.0), Point3F32::new(1.0, 1.0, 1.0));
    let line_f64: Line<Point3> = Line(Point3::new(0.0, 0.0, 0.0), Point3::new(1.0, 1.0, 1.0));

    let query_f32 = Point3F32::new(0.3, 0.3, 0.3);
    let query_f64 = Point3::new(0.3, 0.3, 0.3);

    let mut group = c.benchmark_group("line_presearch");
    group.bench_function("f32_1000div", |b| {
        b.iter(|| {
            v2::algo::curve::presearch(&line_f32, black_box(query_f32), (0.0f32, 1.0f32), 1000)
        })
    });
    group.bench_function("f64_1000div", |b| {
        b.iter(|| {
            v2::algo::curve::presearch(&line_f64, black_box(query_f64), (0.0f64, 1.0f64), 1000)
        })
    });
    group.finish();
}

criterion_group!(benches, bench_evaluate, bench_presearch);
criterion_main!(benches);
