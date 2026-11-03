//! Regression test for planar-cap parameter-curve emission.
//!
//! A cylinder solid has two planar caps whose boundary trim arcs are
//! `Curve::NurbsCurve`s. `compress_with_parameter_curves` must attach a
//! face-local parameter curve to every one of those cap arcs so that a
//! downstream NURBS export can recover the trimmed cap faces. A refactor of
//! `parameter_boundary_2d` dropped the sampled fallback from the `NurbsCurve`
//! arm, which silently stopped emitting cap pcurves (the arcs sit on a
//! `Surface::Plane`, which the exact/direct paths do not handle).

use monstertruck_modeling::*;

/// Builds the era-standard modeling-layer right circular cylinder: revolve a
/// vertex into a closed circle, cap it with a plane, then extrude.
fn cylinder(height: f64, radius: f64) -> Solid {
    let vertex = builder::vertex(Point3::new(0.0, -height / 2.0, radius));
    let circle = builder::revolve(
        &vertex,
        Point3::origin(),
        Vector3::unit_y(),
        builder::SweepAngle::Closed,
        2,
    );
    let disk = builder::try_attach_plane(&[circle]).unwrap();
    builder::extrude(&disk, Vector3::new(0.0, height, 0.0))
}

#[test]
fn cylinder_planar_caps_carry_parameter_curves() {
    let solid = cylinder(1.0, 0.5);
    let tolerance = 0.01;
    let compressed = solid.compress_with_parameter_curves(tolerance);

    assert_eq!(
        compressed.boundaries.len(),
        1,
        "a cylinder solid has a single boundary shell",
    );
    let shell = &compressed.boundaries[0];

    let mut planar_cap_faces = 0usize;
    let mut lateral_faces = 0usize;

    for (face_index, face) in shell.faces.iter().enumerate() {
        let is_cap = matches!(face.surface, Surface::Plane(_));
        if is_cap {
            planar_cap_faces += 1;
        } else {
            lateral_faces += 1;
        }

        for (wire_index, wire) in face.boundaries.iter().enumerate() {
            for (edge_index, edge_use) in wire.iter().enumerate() {
                let edge = &shell.edges[edge_use.index];
                // Every edge-use of every face -- caps included -- must carry a
                // parameter curve. Before the fix, the two `NurbsCurve` arcs of
                // each planar cap came back as `None`.
                let trim_curve = edge_use.trim_curve.as_ref().unwrap_or_else(|| {
                    panic!(
                        "face {face_index} (cap={is_cap}) wire {wire_index} edge {edge_index}: \
                         trim arc (3D curve {:?}) is missing its parameter curve",
                        curve_kind(&edge.curve),
                    )
                });

                // The pcurve must live on this face's own surface.
                assert!(
                    same_surface_kind(trim_curve.surface().as_ref(), &face.surface),
                    "face {face_index}: parameter curve surface does not match the face surface",
                );

                if is_cap {
                    // Cap boundary is made of circular arcs, carried as NURBS.
                    assert!(
                        matches!(edge.curve, Curve::NurbsCurve(_)),
                        "cap face {face_index} trim edge should be a NURBS arc, got {}",
                        curve_kind(&edge.curve),
                    );
                }
            }
        }
    }

    assert_eq!(
        planar_cap_faces, 2,
        "a cylinder has exactly two planar cap faces",
    );
    assert_eq!(
        lateral_faces, 2,
        "the era cylinder splits its lateral surface into two faces \
         (seam + two-division sweep); their seam/trim data must be untouched",
    );
}

fn curve_kind(curve: &Curve) -> &'static str {
    match curve {
        Curve::Line(_) => "Line",
        Curve::BsplineCurve(_) => "BsplineCurve",
        Curve::NurbsCurve(_) => "NurbsCurve",
        Curve::ParameterCurve(_) => "ParameterCurve",
        Curve::IntersectionCurve(_) => "IntersectionCurve",
    }
}

fn same_surface_kind(lhs: &Surface, rhs: &Surface) -> bool {
    matches!(
        (lhs, rhs),
        (Surface::Plane(_), Surface::Plane(_))
            | (Surface::BsplineSurface(_), Surface::BsplineSurface(_))
            | (Surface::NurbsSurface(_), Surface::NurbsSurface(_))
            | (Surface::RevolutionSurface(_), Surface::RevolutionSurface(_))
            | (Surface::TsplineSurface(_), Surface::TsplineSurface(_))
    )
}
