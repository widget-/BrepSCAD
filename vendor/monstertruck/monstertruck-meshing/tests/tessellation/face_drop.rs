//! Spec 007 D1 end-to-end: a face that fails to tessellate must be reported on
//! the loud face-drop metric instead of vanishing silently.
//!
//! This drives the real public tessellation pipeline (`Shell::triangulation`),
//! not the diagnostic helper directly, so it guards the wiring at the
//! face->polygon call site.
//!
//! Spec 007 D1b adds the opt-in STRICT counterpart: `shell_to_polygon_strict`
//! converts the same silent `None`-class drop into a typed
//! `TessellationError::FaceDropped` for correctness-critical callers, while the
//! default `triangulation` / `to_polygon` path stays lenient (drops silently).

use super::*;

/// A shell whose boundary curves deliberately do NOT ride on the surface (the
/// `RobustMeshableShape` doctest shape): the non-robust `triangulation` cannot
/// project the boundary, so its single face drops to `None`.
fn non_riding_boundary_shell() -> Shell {
    let p = [
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(-1.0, 0.0, 0.0),
        Point3::new(1.0, 0.0, 1.0),
        Point3::new(-1.0, 0.0, 1.0),
    ];
    let v = Vertex::from_points(p);
    let bsp0: Curve = BsplineCurve::new(
        KnotVector::bezier_knot(3),
        vec![
            p[0],
            Point3::new(1.0, 4.0 / 3.0, 0.0),
            Point3::new(-1.0, 4.0 / 3.0, 0.0),
            p[1],
        ],
    )
    .into();
    let bsp1: Curve = BsplineCurve::new(
        KnotVector::bezier_knot(3),
        vec![
            p[3],
            Point3::new(-1.0, 4.0 / 3.0, 1.0),
            Point3::new(1.0, 4.0 / 3.0, 1.0),
            p[2],
        ],
    )
    .into();
    let w: Wire = vec![
        builder::line(&v[2], &v[0]),
        Edge::new(&v[0], &v[1], bsp0),
        builder::line(&v[1], &v[3]),
        Edge::new(&v[3], &v[2], bsp1),
    ]
    .into();
    let surface_raw = RevolutionSurface::by_revolution(
        Curve::Line(Line(p[2], p[0])),
        Point3::origin(),
        Vector3::unit_z(),
    );
    let surface: Surface = Processor::new(surface_raw).into();
    vec![Face::new(vec![w], surface)].into()
}

/// A closed unit-cube solid whose every (planar) face meshes cleanly.
fn unit_cube() -> Solid {
    let v = builder::vertex(Point3::origin());
    let e = builder::extrude(&v, Vector3::unit_x());
    let f = builder::extrude(&e, Vector3::unit_y());
    builder::extrude(&f, Vector3::unit_z())
}

/// The non-riding face drops silently under the lenient pipeline. Historically
/// that vanished with no signal; it must now advance [`face_drop_count`].
#[test]
fn non_riding_boundary_drop_is_counted() {
    let shell = non_riding_boundary_shell();

    // The counter is process-global and monotonic within a run; a strict
    // increase across our own dropping tessellation proves the signal fired.
    let before = face_drop_count();
    let poly_shell = shell.triangulation(0.01);

    assert!(
        poly_shell[0].surface().is_none(),
        "a boundary that does not ride on the surface must drop the face to None",
    );
    assert!(
        face_drop_count() > before,
        "a silent face drop must advance the loud face-drop metric",
    );
}

/// D1b: strict meshing converts the same silent `None` drop into a typed
/// `TessellationError::FaceDropped` (the boundary-projection class), instead of
/// quietly returning a mesh missing that face.
#[test]
fn strict_meshing_refuses_a_none_class_face_drop() {
    let shell = non_riding_boundary_shell();

    // Lenient path: the face vanishes silently and `to_polygon` yields a mesh
    // with no faces at all -- the exact silent understatement D1b guards.
    let lenient = shell.triangulation(0.01).to_polygon();
    assert!(
        lenient.faces().is_empty(),
        "the lenient path silently drops the face (empty mesh, no signal)",
    );

    // Strict path: the same drop is a typed refusal naming the face, its surface
    // class, and the reason -- the `None` class is boundary-projection-failed.
    match shell_to_polygon_strict(&shell, 0.01) {
        Err(TessellationError::FaceDropped {
            face,
            surface,
            reason,
        }) => {
            assert_eq!(face, 0, "the single dropped face is index 0");
            assert_eq!(
                reason,
                FaceDropReason::BoundaryProjectionFailed,
                "a non-riding trimmed boundary is the boundary-projection class",
            );
            assert!(
                surface.contains("Surface"),
                "the surface class names the modeling surface type (type_name::<S>), got {surface}",
            );
        }
        other => panic!("strict meshing must refuse the drop, got {other:?}"),
    }
}

/// D1b zero-regression invariant: on a shell whose faces all mesh, strict
/// meshing returns a polygon byte-identical to the lenient `to_polygon` -- same
/// vertex count and identical volume -- so a caller that swaps it in sees no
/// change on a clean shell.
#[test]
fn strict_meshing_matches_lenient_on_a_clean_shell() {
    let cube = unit_cube();
    let shell = &cube.boundaries()[0];

    let strict = shell_to_polygon_strict(shell, 0.01).expect("every cube face meshes cleanly");
    let lenient = shell.triangulation(0.01).to_polygon();

    assert_eq!(
        strict.positions().len(),
        lenient.positions().len(),
        "strict and lenient meshes must have the same vertex count on a clean shell",
    );
    assert_eq!(
        strict.volume().to_bits(),
        lenient.volume().to_bits(),
        "strict and lenient volumes must be byte-identical on a clean shell",
    );
}
