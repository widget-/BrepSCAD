//! Classic-backend boolean tests: the default `and`/`or`/`difference` entry
//! points route through `transversal::classic`. Pins specific to an external
//! SSI backend (named selections, the regression matrix cells, determinism, and
//! the `try_build_solid` healing diagnostics) live with that backend.

use monstertruck_meshing::prelude::*;
use monstertruck_modeling::*;

#[test]
fn adjacent_cubes_or() {
    let v = builder::vertex(Point3::origin());
    let e = builder::extrude(&v, Vector3::unit_x());
    let f = builder::extrude(&e, Vector3::unit_y());
    let cube: Solid = builder::extrude(&f, Vector3::unit_z());

    let v = builder::vertex(Point3::new(0.5, 0.5, 1.0));
    let w = builder::extrude(&v, Vector3::unit_x());
    let f = builder::extrude(&w, Vector3::unit_y());
    let cube2: Solid = builder::extrude(&f, Vector3::unit_z());

    let result = crate::or(&cube, &cube2, 0.05);
    assert!(
        result.is_ok(),
        "Boolean OR of adjacent cubes should succeed: {:?}",
        result.as_ref().err()
    );
    let solid = result.unwrap();

    assert_eq!(solid.boundaries().len(), 1);

    let poly = solid.triangulation(0.01).to_polygon();
    assert_near!(poly.volume(), 2.0);

    let homog = poly.center_of_gravity();
    assert_near!(homog.to_point(), Point3::new(0.75, 0.75, 1.0));

    let bbx = poly.bounding_box();
    assert_near!(bbx.min(), Point3::new(0.0, 0.0, 0.0));
    assert_near!(bbx.max(), Point3::new(1.5, 1.5, 2.0));

    assert_eq!(solid.face_iter().count(), 12);
}

fn matrix_unit_cube() -> Solid {
    let v = builder::vertex(Point3::origin());
    let e = builder::extrude(&v, Vector3::unit_x());
    let f = builder::extrude(&e, Vector3::unit_y());
    builder::extrude(&f, Vector3::unit_z())
}

/// specs/005 cell (Covered): a general crossing box/box AND. The intersection
/// is the box [0.5,1]x[0.25,1]x[0.25,1].
#[test]
fn boolean_matrix_and_crossing_cubes() {
    let cube = matrix_unit_cube();
    let v = builder::vertex(Point3::new(0.5, 0.25, 0.25));
    let e = builder::extrude(&v, Vector3::unit_x());
    let f = builder::extrude(&e, Vector3::unit_y());
    let cube2: Solid = builder::extrude(&f, Vector3::unit_z());

    let solid = crate::and(&cube, &cube2, 0.01).expect("crossing box/box AND must succeed");
    assert_eq!(solid.boundaries().len(), 1);
    assert_eq!(solid.face_iter().count(), 6);
    let poly = solid.triangulation(0.01).to_polygon();
    assert!(
        (poly.volume() - 0.28125).abs() < 1.0e-6,
        "volume {}",
        poly.volume(),
    );
}

#[test]
fn punched_cube() {
    let v = builder::vertex(Point3::origin());
    let e = builder::extrude(&v, Vector3::unit_x());
    let f = builder::extrude(&e, Vector3::unit_y());
    let cube: Solid = builder::extrude(&f, Vector3::unit_z());

    let v = builder::vertex(Point3::new(0.5, 0.25, -0.5));
    let w = builder::revolve(
        &v,
        Point3::new(0.5, 0.5, 0.0),
        Vector3::unit_z(),
        builder::SweepAngle::Closed,
        4,
    );
    let f = builder::try_attach_plane(&[w]).unwrap();
    let mut cylinder = builder::extrude(&f, Vector3::unit_z() * 2.0);
    cylinder.not();
    let and = crate::and(&cube, &cylinder, 0.05).unwrap();
    assert_eq!(and.face_iter().count(), 10);
    assert_eq!(
        and.face_iter()
            .filter(|face| face.absolute_boundaries().len() == 2)
            .count(),
        2
    );

    let poly = and.triangulation(0.01).to_polygon();
    assert!(poly.volume() < 0.9);
}

#[test]
fn cube_clip_high_z() {
    let v = builder::vertex(Point3::origin());
    let e = builder::extrude(&v, Vector3::unit_x());
    let f = builder::extrude(&e, Vector3::unit_y());
    let cube: Solid = builder::extrude(&f, Vector3::unit_z());

    // Clip at z=0.8 with tolerance 0.05 — enough clearance from
    // the cube's top face at z=1.0.
    let clip_box: Solid = primitive::cuboid(BoundingBox::from_iter([
        Point3::new(-1.0, -1.0, -1.0),
        Point3::new(2.0, 2.0, 0.8),
    ]));

    let result = crate::and(&cube, &clip_box, 0.05);
    assert!(
        result.is_ok(),
        "High-Z clip should succeed: {:?}",
        result.as_ref().err()
    );
    let solid = result.unwrap();
    assert_eq!(solid.boundaries().len(), 1);

    let poly = solid.triangulation(0.01).to_polygon();
    assert_near!(poly.volume(), 0.8);
}

#[test]
fn cube_clip_high_z_flip() {
    let v = builder::vertex(Point3::origin());
    let e = builder::extrude(&v, Vector3::unit_x());
    let f = builder::extrude(&e, Vector3::unit_y());
    let cube: Solid = builder::extrude(&f, Vector3::unit_z());

    // Clip at z=0.8 from the top side.
    let clip_box: Solid = primitive::cuboid(BoundingBox::from_iter([
        Point3::new(-1.0, -1.0, 0.8),
        Point3::new(2.0, 2.0, 2.0),
    ]));

    let result = crate::and(&cube, &clip_box, 0.05);
    assert!(
        result.is_ok(),
        "High-Z flip clip should succeed: {:?}",
        result.as_ref().err()
    );
    let solid = result.unwrap();
    assert_eq!(solid.boundaries().len(), 1);

    let poly = solid.triangulation(0.01).to_polygon();
    assert_near!(poly.volume(), 0.2);
}
