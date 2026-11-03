#![cfg(feature = "fillet")]

use monstertruck_modeling::*;

/// Build a simple box shell using extrude, fillet selected edges, verify result.
#[test]
fn fillet_box_edge() {
    let p = [
        Point3::new(0.0, 0.0, 1.0),
        Point3::new(1.0, 0.0, 1.0),
        Point3::new(1.0, 1.0, 1.0),
        Point3::new(0.0, 1.0, 1.0),
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(1.0, 1.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
    ];
    let v: Vec<Vertex> = Vertex::from_points(p);

    let line_edge =
        |i: usize, j: usize| -> Edge { Edge::new(&v[i], &v[j], Curve::Line(Line(p[i], p[j]))) };

    let edge = [
        line_edge(0, 1),
        line_edge(1, 2),
        line_edge(2, 3),
        line_edge(3, 0),
        line_edge(0, 4),
        line_edge(1, 5),
        line_edge(2, 6),
        line_edge(3, 7),
        line_edge(4, 5),
        line_edge(5, 6),
        line_edge(6, 7),
        line_edge(7, 4),
    ];

    let plane_face = |i: usize, j: usize, k: usize, l: usize| -> Face {
        let plane = Plane::new(p[i], p[j], p[l]);
        let wire: Wire = [i, j, k, l]
            .into_iter()
            .zip([i, j, k, l].iter().cycle().skip(1))
            .map(|(a, &b)| {
                edge.iter()
                    .find_map(|e| {
                        if e.front() == &v[a] && e.back() == &v[b] {
                            Some(e.clone())
                        } else if e.back() == &v[a] && e.front() == &v[b] {
                            Some(e.inverse())
                        } else {
                            None
                        }
                    })
                    .unwrap()
            })
            .collect();
        Face::new(vec![wire], Surface::Plane(plane))
    };

    let mut shell: Shell = [
        plane_face(0, 1, 2, 3),
        plane_face(1, 0, 4, 5),
        plane_face(2, 1, 5, 6),
        plane_face(3, 2, 6, 7),
    ]
    .into();

    let initial_face_count = shell.len();

    let params = FilletOptions {
        radius: RadiusSpec::Constant(0.4),
        ..Default::default()
    };
    fillet_edges(&mut shell, &[edge[5].clone()], Some(&params)).unwrap();

    assert!(shell.len() > initial_face_count);
}

/// A shell containing a curve with no exact NURBS representation must refuse
/// with a typed error, not silently resample it into a polyline NURBS.
#[test]
fn fillet_refuses_non_exact_curve_typed() {
    let p = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(1.0, 1.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
    ];
    let v: Vec<Vertex> = Vertex::from_points(p);
    let plane = Plane::new(p[0], p[1], p[3]);
    // A parameter curve (pcurve on the plane) tracing the bottom edge: it has
    // no exact NURBS conversion, so filleting must refuse typed.
    let pcurve = Curve::ParameterCurve(ParameterCurve::new(
        Curve2D::Line(Line(Point2::new(0.0, 0.0), Point2::new(1.0, 0.0))),
        Box::new(Surface::Plane(plane)),
    ));
    let edge = [
        Edge::new(&v[0], &v[1], pcurve),
        Edge::new(&v[1], &v[2], Curve::Line(Line(p[1], p[2]))),
        Edge::new(&v[2], &v[3], Curve::Line(Line(p[2], p[3]))),
        Edge::new(&v[3], &v[0], Curve::Line(Line(p[3], p[0]))),
    ];
    let wire: Wire = edge.iter().cloned().collect();
    let face = Face::new(vec![wire], Surface::Plane(plane));
    let mut shell: Shell = vec![face].into();

    let params = FilletOptions {
        radius: RadiusSpec::Constant(0.1),
        ..Default::default()
    };
    let result = fillet_edges(&mut shell, &edge[..1], Some(&params));
    assert!(
        matches!(result, Err(FilletError::UnsupportedGeometry { .. })),
        "expected typed refusal, got {result:?}"
    );
}
