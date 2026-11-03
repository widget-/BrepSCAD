use super::*;

/// Returns a result which provides information regarding the connection of two points on
/// `point`'s connection in the direction `dir`.
///
/// # Returns.
/// - `(0, ERROR)` when `point`'s connection is invalid.
/// - `(1, ERROR)` when `other`'s connection is invalid.
///
/// - `(#, TmeshConnectionNotFound)` when the connection is a T-mesh.
/// - `(#, TmeshControlPointNotFound)` when the connection is an edge condition.
/// - `(#, TmeshExistingConnection)` when the connection does not lead to the correct point.
///
/// - `Ok(())` if the connection is correctly configured.
fn test_points_are_connected<P: PartialEq>(
    point: Arc<RwLock<TmeshControlPoint<P>>>,
    other: Arc<RwLock<TmeshControlPoint<P>>>,
    dir: TmeshDirection,
) -> std::result::Result<(), (i32, Error)> {
    // Check that point is connected to other
    let point_borrow = point.read();
    let point_con = &point_borrow.try_connected_point(dir).map_err(|e| (0, e))?;
    let point_equal = Arc::ptr_eq(point_con, &other);
    point_equal
        .then_some(0)
        .ok_or((0, Error::TmeshExistingConnection))?;

    // Check that other is connected to point
    let other_borrow = other.read();
    let other_con = &other_borrow
        .try_connected_point(dir.flip())
        .map_err(|e| (1, e))?;
    let other_equal = Arc::ptr_eq(other_con, &point);
    other_equal
        .then_some(0)
        .ok_or((1, Error::TmeshExistingConnection))?;
    Ok(())
}

/// Tests the construction of a new T-mesh, verifying that all the points are correctly connected and exist.
#[test]
fn test_t_mesh_new() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 0.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 0.0)),
    ];

    let mesh = Tmesh::new(points, 1.0);

    // Test that there are four control points in the mesh after creation.
    assert!(
        mesh.control_points().len() == 4,
        "T-mesh retained {} of 4 points during creation",
        mesh.control_points.len(),
    );

    // Test that the origin and the right are correctly connected
    let con_test = test_points_are_connected(
        mesh.find(Point3::from((0.0, 0.0, 0.0))).unwrap(),
        mesh.find(Point3::from((1.0, 0.0, 0.0))).unwrap(),
        TmeshDirection::Right,
    );
    assert!(
        con_test.is_ok(),
        "The origin is not correctly connected to (1, 0, 0)"
    );

    // Test that the origin and the up are correctly connected
    let con_test: std::result::Result<(), (i32, Error)> = test_points_are_connected(
        mesh.find(Point3::from((0.0, 0.0, 0.0))).unwrap(),
        mesh.find(Point3::from((0.0, 1.0, 0.0))).unwrap(),
        TmeshDirection::Up,
    );
    assert!(
        con_test.is_ok(),
        "The origin is not correctly connected to (0, 1, 0)"
    );

    // Test that (1,1,0) and the up are correctly connected
    let con_test: std::result::Result<(), (i32, Error)> = test_points_are_connected(
        mesh.find(Point3::from((1.0, 1.0, 0.0))).unwrap(),
        mesh.find(Point3::from((0.0, 1.0, 0.0))).unwrap(),
        TmeshDirection::Left,
    );
    assert!(
        con_test.is_ok(),
        "(1, 1, 0) is not correctly connected to (0, 1, 0)"
    );

    // Test that (1,1,0) and the right are correctly connected
    let con_test: std::result::Result<(), (i32, Error)> = test_points_are_connected(
        mesh.find(Point3::from((1.0, 1.0, 0.0))).unwrap(),
        mesh.find(Point3::from((1.0, 0.0, 0.0))).unwrap(),
        TmeshDirection::Down,
    );
    assert!(
        con_test.is_ok(),
        "(1, 1, 0) is not correctly connected to (1, 0, 0)"
    );
}

/// Constructs a T-mesh, testing that inserting a new control point with no inferred connections
/// produces the correct result.
///
/// ```
///    |  |  |
///  --+-[+]-+--
///    |     |
///  --+-----+--
///    |     |
/// ```
/// `[+]` is the inserted control point, which is tested. Testing includes verifying conenctions to other points,
/// making sure the T-junction in the `DOWN` direction is correct, and verifying the edge condition.
#[test]
fn test_t_mesh_insert_control_point() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 0.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 0.0)),
    ];

    let mut mesh = Tmesh::new(points, 1.0);

    mesh.add_control_point(
        Point3::from((0.5, 1.0, 0.0)),
        mesh.find(Point3::from((0.0, 1.0, 0.0)))
            .expect("Point (0, 1, 0) is a valid point in the T-mesh"),
        TmeshDirection::Right,
        0.5,
    )
    .expect("Arguments provided to add_control_point are valid and insertion is allowed");

    let top_left = mesh.find(Point3::from((0.0, 1.0, 0.0))).unwrap();
    let top_mid = mesh.find(Point3::from((0.5, 1.0, 0.0))).unwrap();
    let top_right = mesh.find(Point3::from((1.0, 1.0, 0.0))).unwrap();

    // Test that there are five control points in the mesh after insertion.
    assert!(
        mesh.control_points().len() == 5,
        "Inserted control point was not added to control_points vector"
    );

    // Test that the left and the middle are correctly connected
    let left_mid_con = test_points_are_connected(
        Arc::clone(&top_left),
        Arc::clone(&top_mid),
        TmeshDirection::Right,
    );
    assert!(
        left_mid_con.is_ok(),
        "Top left and top middle points are not correctly connected"
    );

    // Test that the right and the middle are correctly connected
    let right_mid_con = test_points_are_connected(
        Arc::clone(&top_right),
        Arc::clone(&top_mid),
        TmeshDirection::Left,
    );
    assert!(
        right_mid_con.is_ok(),
        "Top left and top middle points are not correctly connected"
    );

    // Check edge condition for the middle
    assert!(
        top_mid
            .read()
            .get(TmeshDirection::Up)
            .as_ref()
            .is_some_and(|c| c.0.is_none() && (c.1 - 1.0).so_small()),
        "Top middle edge condition (direction UP) is incorrectly configured"
    );

    // Check T-junction for the middle
    assert!(
        top_mid.read().get(TmeshDirection::Down).is_none(),
        "Top middle T-junction (direction DOWN) is incorrectly configured"
    );
}

/// Constructs a T-mesh, testing that inserting a new control point with one inferred connection
/// produces the correct result.
///
/// ```
///    |  |  |
///  --+--+--+--
///    |  :  |
///  --+-[+]-+--
///    |  |  |
/// ```
/// `[+]` is the inserted control point, which is tested. The control point is inserted on the `RIGHT`
/// connection of the bottom left point, and the connection marked `:` is the inferred connection which
/// should exist after `[+]` is inserted.
#[test]
fn test_t_mesh_insert_control_point_one_inferred_connection() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 0.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 0.0)),
    ];

    let mut mesh = Tmesh::new(points, 1.0);

    // Add the first control points
    mesh.add_control_point(
        Point3::from((0.5, 1.0, 0.0)),
        mesh.find(Point3::from((0.0, 1.0, 0.0)))
            .expect("Point (0, 1, 0) is a valid point in the T-mesh"),
        TmeshDirection::Right,
        0.5,
    )
    .expect("Arguments provided to add_control_point are valid and insertion is allowed");

    // Add second control point with inferred connection
    mesh.add_control_point(
        Point3::from((0.5, 0.0, 0.0)),
        mesh.find(Point3::from((0.0, 0.0, 0.0)))
            .expect("Point (0, 0, 0) is a valid point in the T-mesh"),
        TmeshDirection::Right,
        0.5,
    )
    .expect("Arguments provided to add_control_point are valid and insertion is allowed");

    let top_mid = mesh.find(Point3::from((0.5, 1.0, 0.0))).unwrap();
    let bottom_mid = mesh.find(Point3::from((0.5, 0.0, 0.0))).unwrap();

    // Test that the inferrect connection exists
    let inferred_con_exist = test_points_are_connected(
        Arc::clone(&bottom_mid),
        Arc::clone(&top_mid),
        TmeshDirection::Up,
    );
    assert!(
        inferred_con_exist.is_ok(),
        "Inferred connection is not correctly configured"
    );

    // Test that inferred connection knot interval is correctly configured
    let inferred_con_interval = {
        let borrow = top_mid.read();

        (borrow
            .connection_knot(TmeshDirection::Down)
            .expect("Connection should exist")
            - 1.0)
            .so_small()
    };
    assert!(
        inferred_con_interval,
        "Inferred connection knot interval is incorrect"
    );
}

/// Tests to make sure that a mesh with the following shape is correctly formed and connected. Knot intervals may be arbitrary,
/// however, cartesian points must be located on a 0.5 spaced grid with a 0 z-coordinate. Thus, the center point is
/// located at `(0.5, 0.5, 0)` and so on.
/// ```
///    |  |  |
///  --+--+--+--
///    |  |  |
///  --+--+--+--
///    |  |  |
///  --+--+--+--
///    |  |  |
/// ```
fn test_t_mesh_plus_mesh(mesh: &Tmesh<Point3>) {
    let middle = mesh.find(Point3::from((0.5, 0.5, 0.0))).unwrap();

    // Test connections in the four directions
    let up_con = test_points_are_connected(
        Arc::clone(&middle),
        Arc::clone(&mesh.find(Point3::from((0.5, 1.0, 0.0))).unwrap()),
        TmeshDirection::Up,
    );
    assert!(up_con.is_ok(), "Middle is not correctly connected to UP");

    let right_con = test_points_are_connected(
        Arc::clone(&middle),
        Arc::clone(&mesh.find(Point3::from((1.0, 0.5, 0.0))).unwrap()),
        TmeshDirection::Right,
    );
    assert!(
        right_con.is_ok(),
        "Middle is not correctly connected to RIGHT"
    );

    let down_con = test_points_are_connected(
        Arc::clone(&middle),
        Arc::clone(&mesh.find(Point3::from((0.5, 0.0, 0.0))).unwrap()),
        TmeshDirection::Down,
    );
    assert!(
        down_con.is_ok(),
        "Middle is not correctly connected to DOWN"
    );

    let left_con = test_points_are_connected(
        Arc::clone(&middle),
        Arc::clone(&mesh.find(Point3::from((0.0, 0.5, 0.0))).unwrap()),
        TmeshDirection::Left,
    );
    assert!(
        left_con.is_ok(),
        "Middle is not correctly connected to LEFT"
    );
}

/// Constructs a T-mesh, testing that inserting a new control point with two inferred connections
/// produces the correct result. Utilizes the `add_control_point` function for point insertion.
///
/// ```
///    |  |  |
///  --+-<+>-+--
///    |  |  |
///  --+~[+]~+--
///    |  |  |
///  --+--+--+--
///    |  |  |
/// ```
/// `[+]` is the inserted control point, which is tested. The control point is inserted on the `DOWN` connection of
/// `<+>`, and the connections marked `~` are inferred connections which should exist after `[+]` is inserted.
#[test]
fn test_t_mesh_insert_control_point_two_inferred_connections() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 0.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 0.0)),
    ];

    let mut mesh = Tmesh::new(points, 1.0);

    // Add the four control points
    let points = [
        ((0.5, 0.0, 0.0), (0.0, 0.0, 0.0)), // bottom mid, connects to (0, 0, 0) from its right
        ((1.0, 0.5, 0.0), (1.0, 0.0, 0.0)), // right mid,  connects to (1, 0, 0) from its up
        ((0.5, 1.0, 0.0), (1.0, 1.0, 0.0)), // top mid,    connects to (1, 1, 0) from its left
        ((0.0, 0.5, 0.0), (0.0, 1.0, 0.0)), // right mid,  connects to (0, 1, 0) from its down
    ];
    let mut dir = TmeshDirection::Right;

    for point_pair in points {
        mesh.add_control_point(
            Point3::from(point_pair.0),
            mesh.find(Point3::from(point_pair.1)).unwrap_or_else(|_| {
                panic!("Point {:?} is a valid point in the T-mesh", point_pair.1)
            }),
            dir,
            0.5,
        )
        .expect("Arguments provided to add_control_point are valid and insertion is allowed");
        dir = dir.anti_clockwise();
    }

    // Add center control point with inferred connections
    mesh.add_control_point(
        Point3::from((0.5, 0.5, 0.0)),
        mesh.find(Point3::from((0.5, 0.0, 0.0)))
            .expect("Point (0.5, 0, 0) is a valid point in the T-mesh"),
        TmeshDirection::Up,
        0.5,
    )
    .expect("Arguments provided to add_control_point are valid and insertion is allowed");

    test_t_mesh_plus_mesh(&mesh);
}

/// Constructs a T-mesh, testing that inserting a new control point with two inferred connections
/// produces the correct result. Utilizes the `try_add_absolute_point` function for point insertion.
///
/// ```
///    |  |  |
///  --+-<+>-+--
///    |  |  |
///  --+~[+]~+--
///    |  |  |
///  --+--+--+--
///    |  |  |
/// ```
/// `[+]` is the inserted control point, which is tested. The control point is inserted on the `DOWN` connection of
/// `<+>`, and the connections marked `~` are inferred connections which should exist after `[+]` is inserted.
#[test]
fn test_t_mesh_try_add_absolute_point_mesh_construction() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 0.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 0.0)),
    ];

    let mut mesh = Tmesh::new(points, 1.0);

    // Insert vertical aspect of the plus
    mesh.try_add_absolute_point(Point3::from((0.0, 0.5, 0.0)), (0.0, 0.5))
        .expect("Legal point insertion");
    mesh.try_add_absolute_point(Point3::from((1.0, 0.5, 0.0)), (1.0, 0.5))
        .expect("Legal point insertion");

    // Insert horizontal aspect of the plus
    mesh.try_add_absolute_point(Point3::from((0.5, 0.0, 0.0)), (0.5, 0.0))
        .expect("Legal point insertion");
    mesh.try_add_absolute_point(Point3::from((0.5, 1.0, 0.0)), (0.5, 1.0))
        .expect("Legal point insertion");

    // Insert center point of the plus
    mesh.try_add_absolute_point(Point3::from((0.5, 0.5, 0.0)), (0.5, 0.5))
        .expect("Legal point insertion");

    test_t_mesh_plus_mesh(&mesh);
}

/// Constructs the following T-mesh, testing that inserting a new control point using
/// `try_add_absolute_point` function produces a point with the correct knot intervals.
///
/// ```
///    |       |
///  --+-------+--
///    |       |
///  --+-+-----+--
///    | |     |
/// ```
#[test]
fn test_t_mesh_try_add_absolute_point_knot_intervals() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 0.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 0.0)),
    ];

    let mut mesh = Tmesh::new(points, 1.0);
    mesh.try_add_absolute_point(Point3::from((0.2, 0.0, 0.0)), (0.2, 0.0))
        .expect("Legal point insertion");

    // Insert a point asymetrically into a mesh to test if knot interval calculations work
    let knot_interval_check = mesh
        .find(Point3::from((0.2, 0.0, 0.0)))
        .expect("Control point previously inserted into mesh");

    // Left connection should be connected to (0, 0, 0), with interval 0.2
    assert_eq!(
        knot_interval_check
            .read()
            .connection_knot(TmeshDirection::Left)
            .expect("Known existing connection"),
        0.2,
        "Knot interval on LEFT does not match expectation"
    );

    // Right connection should be connected to (1, 0, 0), with interval 0.8
    assert_eq!(
        knot_interval_check
            .read()
            .connection_knot(TmeshDirection::Right)
            .expect("Known existing connection"),
        0.8,
        "Knot interval on RIGHT does not match expectation"
    );
}

/// Constructs a T-mesh, testing that inserting a new control point using
/// `try_add_absolute_point` function produces errors when attempting to insert an unconnected point,
/// an existing point, and an out-of-bound point.
///
/// ```            
///             {+}
///    |   |     
///  --+---+--
///    |[+]|
///  -<+>--+--
///    |   |
/// ```
/// <+> is the duplicate point
/// [+] is the unconnected pont
/// {+} is the out-of-bounds point
#[test]
fn test_t_mesh_try_add_absolute_point_invalid_insertion() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 0.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 0.0)),
    ];

    let mut mesh = Tmesh::new(points, 1.0);

    // Test errors on inserting a point into the center of a face (unconnected point)
    assert!(
        mesh.try_add_absolute_point(Point3::from((0.5, 0.5, 0.0)), (0.5, 0.5))
            .is_err_and(|e| { e == Error::TmeshConnectionNotFound }),
        "Expected Error TmeshConnectionNotFound when attempting to insert a point in a location with no intersecting mesh edges."
    );

    // Test errors on zero intervals (duplicate point)
    assert!(
        mesh.try_add_absolute_point(Point3::from((0.0, 0.0, 0.0)), (0.0, 0.0))
            .is_err_and(|e| { e == Error::TmeshExistingControlPoint }),
        "Expected Error TmeshExistingControlPoint when attempting to insert a point in a location where a control point already exists."
    );

    // Test errrors on out-of-bounds insertions.
    assert!(
        mesh.try_add_absolute_point(Point3::from((2.0, 2.0, 0.0)), (2.0, 2.0))
            .is_err_and(|e| { e == Error::TmeshOutOfBoundsInsertion }),
        "Expected Error TmeshOutOfBoundsInsertion when attempting to insert a point outside the parametric domain of the mesh."
    );
}

/// Constructs the following T-mesh, testing that navigating from the origin to a connection in the
/// right direction functions as expected.
///
/// ```
///    |      |
///  --+------+--
///    |      |
///  --+      |
///    |      |
///  --+------+--
///    |      |
/// ```
/// <+> is the duplicate point
/// [+] is the unconnected pont
/// {+} is the out-of-bounds point
#[test]
fn test_t_mesh_navigate_until_con_existing_con() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 0.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 0.0)),
    ];

    let mut mesh = Tmesh::new(points, 1.0);
    let origin = mesh
        .find(Point3::from((0.0, 0.0, 0.0)))
        .expect("Point exists in T-mesh");

    // Add control point for navigation
    mesh.add_control_point(
        Point3::from((0.0, 0.5, 0.0)),
        Arc::clone(&origin),
        TmeshDirection::Up,
        0.5,
    )
    .expect("Valid addition of control point.");

    // Navigates to the top left point
    let navigation_result = origin
        .read()
        .navigate_until_con(TmeshDirection::Up, TmeshDirection::Right);

    assert!(
        navigation_result.is_ok(),
        "Error navigating until existing connecton"
    );
    assert_eq!(
        navigation_result.as_ref().unwrap().0.read().point,
        Point3::from((0.0, 1.0, 0.0)),
        "Navigation returned incorrect point"
    );
    assert_eq!(
        navigation_result.as_ref().unwrap().1,
        1.0,
        "Navigation knot interval incorrect"
    );
}

/// Constructs the following T-mesh, testing that navigating from the origin to a connection in the
/// left direction returns an error.
///
/// ```
///    |      |
///  --+------+--
///    |      |
///  --+      |
///    |      |
///  --+------+--
///    |      |
/// ```
/// <+> is the duplicate point
/// [+] is the unconnected pont
/// {+} is the out-of-bounds point
#[test]
fn test_t_mesh_navigate_until_con_no_existing_con() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 0.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 0.0)),
    ];

    let mut mesh = Tmesh::new(points, 1.0);
    let origin = mesh
        .find(Point3::from((0.0, 0.0, 0.0)))
        .expect("Point exists in T-mesh");

    // Add control point for navigation
    mesh.add_control_point(
        Point3::from((0.0, 0.5, 0.0)),
        Arc::clone(&origin),
        TmeshDirection::Up,
        0.5,
    )
    .expect("Valid addition of control point.");

    // Navigate until error
    let navigation_result = origin
        .read()
        .navigate_until_con(TmeshDirection::Up, TmeshDirection::Left);

    assert!(
        navigation_result.is_err(),
        "Navigation to non-existant connection succeeded (Should have failed)"
    );
    assert_eq!(
        navigation_result.as_ref().err(),
        Some(&Error::TmeshControlPointNotFound),
        "Expected TmeshControlPointNotFound, got {:?}",
        navigation_result.as_ref().err()
    );
}

/// Constructs the following (unsolvable) T-mesh, with the knot coordinates specified on the left and bottom. All edge condition
///  intervals have a knot interval of 2.5.
///
/// ```
///  1.0   +-----+-----------------------------------+
///        |     |                                   |
///  0.9   |     +-------+---+---+-----+---+-----+---+
///        |     |       |   |   |     |   |     |   |
///  0.8   |     |       |   |   |     +---+     |   |
///        |     |       |   |   |     |   |     |   |
///  0.7   |     |       +---+---+     |   +     |   |
///  0.6   |     |       |       |     |   |     +---+
///  0.5   |     |       |       +-----+   |     |   |
///  0.4   +     |       +       |     |   |     |   +
///  0.3   |     +-------+       |     |   |     |   |
///  0.2   |     |       |       +-----+---+-----+---+
///        |     |       |       |                   |
///  0.0   +-----+-------+-------+-------------------+
///       0.0   0.2     0.3 0.4 0.5  0.6  0.7   0.9 1.0
/// ```
fn construct_ray_casting_example_mesh() -> Tmesh<Point3> {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 0.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 0.0)),
    ];

    let mut mesh = Tmesh::new(points, 2.5);

    // Absolute knot coordinatess of the points from the mesh above. They are ordered such that the
    // edges in the above image will be constructed without conflict, and so that points are only
    // inserted on existing edges.
    let knot_pairs = Vec::from([
        (0.0, 0.4),
        (0.2, 1.0),
        (1.0, 0.9),
        (1.0, 0.6),
        (1.0, 0.2),
        (0.5, 0.0),
        (0.3, 0.0),
        (0.2, 0.0),
        (0.2, 0.3),
        (0.2, 0.9),
        (0.3, 0.9),
        (0.4, 0.9),
        (0.5, 0.9),
        (0.6, 0.9),
        (0.7, 0.9),
        (0.9, 0.9),
        (0.3, 0.7),
        (0.3, 0.4),
        (0.3, 0.3),
        (0.5, 0.7),
        (0.5, 0.5),
        (0.5, 0.2),
        (0.4, 0.7),
        (0.6, 0.2),
        (0.7, 0.2),
        (0.9, 0.2),
        (0.6, 0.5),
        (0.6, 0.8),
        (0.7, 0.7),
        (0.7, 0.8),
        (0.9, 0.6),
        (1.0, 0.4),
    ]);

    // Construct mesh
    for knot_pair in knot_pairs {
        mesh.try_add_absolute_point(Point3::from((knot_pair.0, knot_pair.1, 0.0)), knot_pair)
            .unwrap_or_else(|_| {
                panic!(
                    "Valid addition of control point ({}, {}).",
                    knot_pair.0, knot_pair.1
                )
            });
    }

    mesh
}

/// Tests if the face intersection algorithm in cast_ray functions as expected, including testing if the
/// point-edge detection, but not connection traversal, aspect of the algorithm function as expected. Uses the mesh
/// constructed by construct_ray_casting_example_mesh to test cast_ray, by casting a ray from the point
/// located at (0.0, 0.4) in parametric space in the direction RIGHT.
#[test]
fn test_t_mesh_ray_casting_face_intersection() {
    // Construct mesh
    let mesh = construct_ray_casting_example_mesh();

    // Select the initial point
    let start = mesh
        .find(Point3::from((0.0, 0.4, 0.0)))
        .expect("Known existing point in mesh");

    // Cast ray
    let intersections = Tmesh::cast_ray(Arc::clone(&start), TmeshDirection::Right, 9);

    assert!(
        intersections.is_ok(),
        "Ray casting produces unexpectd error"
    );
    let intersections = intersections.unwrap();

    // Because 9 intersections are requested in the cast_ray function call, the returned vector must be of length 9
    assert_eq!(
        intersections.len(),
        9,
        "The incorrect number of intervals was returned"
    );

    // Check values in the returned vector.
    assert!(
        intersections
            .iter()
            .zip(Vec::from([0.2, 0.1, 0.2, 0.1, 0.1, 0.2, 0.1, 2.5, 0.0]))
            .all(|p| (p.0 - p.1).so_small()),
        "Recorded knot intervals differ form expectation"
    );
}

/// Tests if the face intersection algorithm in cast_ray functions as expected. Does not test if the
/// edge detection or connection traversal aspects of the algorithm function as expected, however,
/// it does test if the T-junction traversal algorithm terminates when expected. Uses the mesh
/// constructed by construct_ray_casting_example_mesh to test cast_ray, by casting a ray from the point
/// located at (0.0, 0.4) in parametric space in the direction RIGHT.
#[test]
fn test_t_mesh_ray_casting_face_intersection_incomplete_cast() {
    // Construct mesh
    let mesh = construct_ray_casting_example_mesh();

    // Select the initial point
    let start = mesh
        .find(Point3::from((0.0, 0.4, 0.0)))
        .expect("Known existing point in mesh");

    // Cast ray
    let intersections = Tmesh::cast_ray(Arc::clone(&start), TmeshDirection::Right, 5);

    assert!(
        intersections.is_ok(),
        "Ray casting produces unexpectd error"
    );
    let intersections = intersections.unwrap();

    // Because 9 intersections are requested in the cast_ray function call, the returned vector must be of length 9
    assert_eq!(
        intersections.len(),
        5,
        "The incorrect number of intervals was returned"
    );

    // Check values in the returned vector.
    assert!(
        intersections
            .iter()
            .zip(Vec::from([0.2, 0.1, 0.2, 0.1, 0.1]))
            .all(|p| (p.0 - p.1).so_small()),
        "Recorded knot intervals differ form expectation"
    );
}

/// Tests if the face intersection algorithm in cast_ray functions as expected, including testing if the
/// T-junction edge detection and connection traversal aspects of the algorithm function as expected. Uses the mesh
/// constructed by construct_ray_casting_example_mesh to test cast_ray, by casting a ray from the point
/// located at (0.0, 0.4) in parametric space in the direction RIGHT.
#[test]
fn test_t_mesh_ray_casting_non_point_edge_condition() {
    // Construct mesh
    let mesh = construct_ray_casting_example_mesh();

    // Select the initial point
    let start = mesh
        .find(Point3::from((0.3, 0.7, 0.0)))
        .expect("Known existing point in mesh");

    // Cast ray
    let intersections = Tmesh::cast_ray(Arc::clone(&start), TmeshDirection::Right, 8);

    assert!(
        intersections.is_ok(),
        "Ray casting produces unexpectd error"
    );
    let intersections = intersections.unwrap();
    println!("{:?}", intersections);
    // Because 9 intersections are requested in the cast_ray function call, the returned vector must be of length 9
    assert_eq!(
        intersections.len(),
        8,
        "The incorrect number of intervals was returned"
    );

    // Check values in the returned vector.
    assert!(
        intersections
            .iter()
            .zip(Vec::from([0.1, 0.1, 0.1, 0.1, 0.2, 0.1, 2.5, 0.0]))
            .all(|p| (p.0 - p.1).so_small()),
        "Recorded knot intervals differ form expectation"
    );
}

/// Clones the mesh produced by `construct_ray_casting_example_mesh` and then compares it to a second,
/// uncloned mesh from `construct_ray_casting_example_mesh`.
#[test]
fn test_t_mesh_clone() {
    let tmesh_test = construct_ray_casting_example_mesh().clone();
    let tmesh_comp = construct_ray_casting_example_mesh();

    // Test number of control points
    assert!(
        tmesh_test.control_points().len() == tmesh_comp.control_points().len(),
        "Number of control points in mesh is not the same as original mesh"
    );

    // Test cartesian points
    assert!(
        tmesh_test
            .control_points()
            .iter()
            .zip(tmesh_comp.control_points().iter())
            .all(|p| { p.0.read().point() == p.1.read().point() }),
        "Control points of cloned mesh are not the same as original mesh"
    );

    // Test parametric points
    assert!(
        tmesh_test
            .control_points()
            .iter()
            .zip(tmesh_comp.control_points().iter())
            .all(|p| { p.0.read().knot_coordinates() == p.1.read().knot_coordinates() }),
        "Parametric coordinates of cloned mesh are not the same as original mesh"
    );

    // Test connections
    assert!(
        tmesh_test
            .control_points()
            .iter()
            .zip(tmesh_comp.control_points().iter())
            .all(|p| {
                // Test all directions of every point in the meshes
                for dir in TmeshDirection::iter() {
                    // Compare connection types
                    if p.0.read().con_type(dir) != p.1.read().con_type(dir) {
                        return false;
                    }

                    // Based on the conenction type, compare connected objects
                    match p.0.read().con_type(dir) {
                        TmeshConnectionType::Edge => {
                            // Compare knot intervals
                            if p.0.read().connection_knot(dir) != p.1.read().connection_knot(dir) {
                                return false;
                            }
                        }
                        TmeshConnectionType::Point => {
                            // Compare knot intervals
                            if p.0.read().connection_knot(dir) != p.1.read().connection_knot(dir) {
                                return false;
                            }

                            // Get connection object from both meshes
                            let test_borrow = p.0.read();
                            let test_con = test_borrow
                                .get(dir)
                                .as_ref()
                                .expect("Point con type must have a connection");
                            let comp_borrow = p.1.read();
                            let comp_con = comp_borrow
                                .get(dir)
                                .as_ref()
                                .expect("Point con type must have a connection");

                            // Compare connected points
                            if test_con
                                .0
                                .as_ref()
                                .expect("Point con type must have a point connected")
                                .read()
                                .point()
                                != comp_con
                                    .0
                                    .as_ref()
                                    .expect("Point con type must have a point connected")
                                    .read()
                                    .point()
                            {
                                return false;
                            }
                        }
                        TmeshConnectionType::Tjunction => {}
                    }
                }

                true
            })
    )
}

/// Creates a plane of the form `x + y = z` and solves it using `subs`.
#[test]
fn test_t_mesh_subs() {
    const C: usize = 100;
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 1.0)),
        Point3::from((1.0, 1.0, 2.0)),
        Point3::from((0.0, 1.0, 1.0)),
    ];

    // Tmesh is now the surface x + y = z
    let mesh = Tmesh::new(points, 1.0);

    for s in 0..C {
        let s = s as f64 / C as f64;
        for t in 0..C {
            let t = t as f64 / C as f64;
            let p = mesh
                .subs(s, t)
                .expect("Solvable T-mesh with s and t within bounds");

            assert!(
                ((p.x + p.y) - p.z).so_small(),
                "Returned subs value does not match expectation."
            );
        }
    }
}

/// Returns a point half-way between `a` and `b`.
fn average_points(a: Point3, b: Point3) -> Point3 { 0.5 * (a + ControlPoint::to_vec(b)) }

/// Subdivides a T-mesh from a two by two into a three by three, checking that the connections and knot vectors
/// are correct. Does not check if control points are correctly spaced in cartesian space, since that is calculated
/// with a caller provided function.
#[test]
fn test_t_mesh_subdivide() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 1.0)),
        Point3::from((1.0, 1.0, 2.0)),
        Point3::from((0.0, 1.0, 1.0)),
    ];

    // Tmesh is now a surface where all point on the surface are of the form (f(x), g(y), f(x) + g(y))
    // approximates x + y = z with medial x and y values
    let mut mesh = Tmesh::new(points, 1.0);

    // Subdivision should be successful
    let sub_res = mesh.subdivide(average_points);
    assert!(
        sub_res.is_ok(),
        "Error while subdividing mesh {}.",
        sub_res.err().unwrap()
    );

    // Mesh becomes a 3x3 grid, 9 control points
    assert_eq!(
        mesh.control_points().len(),
        9,
        "Incorrect number of control points found in the subdivided mesh."
    );

    // Test middle point for inifered connection shenanegins
    let middle_point = mesh
        .find(Point3::from((0.5, 0.5, 1.0)))
        .expect("Control point should be located in subdivided mesh");
    for dir in TmeshDirection::iter() {
        assert_eq!(
            middle_point.read().con_type(dir),
            TmeshConnectionType::Point,
            "Expected a point connection in the direction {}.",
            dir
        );
        assert!(
            (middle_point.read().connection_knot(dir).unwrap() - 0.5).so_small(),
            "Expected knot interval of 0.5."
        );
    }

    // Make sure each point still follows the x + y = z scheme (averaging will have no effect on this)
    for point in mesh.control_points.iter() {
        let p = *point.read().point();
        assert!(
            (p.x + p.y - p.z).so_small(),
            "Point does not follow expected cartesian scheme."
        );
    }
}

/// Checks if two `Point3` instances are eqaul using tollerance.
fn points_eq(a: Point3, b: Point3) -> bool { (a.x + a.y + a.z - (b.x + b.y + b.z)).so_small() }

/// Test legal local knot insertion by creating two identical surfaces, then performing LKI on one of
/// them and checking with `subs` if the surfaces differ. In order to maximize any differences between the
/// surfaces, the control points which are affected by the LKI are moved such that they have no lienear
/// realtion between them in any axis. The cartesian space of the coordinates is cross-referenced with manually
/// performed mathematics, which can be seen in desmos links in some of the inline comments on top of the manual
/// confirmation through the use of `subs`.
#[test]
fn test_t_mesh_local_knot_insertion_no_edge_conditions() {
    const N: usize = 25;
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 1.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 1.0)),
    ];

    let mut mesh = Tmesh::new(points, 1.0);

    // Make mesh a 5x5
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");

    // Modify mesh so that the form is highly dependant on all elements of a point. Nescessary because if the control points
    // are on a (flat) plane, then the elements which change due to LKI (x and y) can be almost anything and the limit surface
    // will be the same. If the points are more scattered, then deviation in elements which get canceled out by the "averging"
    // nature of the LKI algorithm will become more evident in the elements which are not "averaged out".
    mesh.map_point(
        Point3::from((0.25, 0.25, 0.375)),
        Point3::from((0.25, 0.10, 0.375)),
    )
    .expect("Control point is in mesh");
    mesh.map_point(
        Point3::from((0.50, 0.25, 0.500)),
        Point3::from((0.50, 0.30, 0.300)),
    )
    .expect("Control point is in mesh");
    mesh.map_point(
        Point3::from((0.75, 0.25, 0.625)),
        Point3::from((0.75, 0.15, 0.625)),
    )
    .expect("Control point is in mesh");
    mesh.map_point(
        Point3::from((1.00, 0.25, 0.750)),
        Point3::from((1.00, 0.25, 0.200)),
    )
    .expect("Control point is in mesh");

    let mut test = mesh.clone();

    let ins_point = test
        .try_local_knot_insertion(
            test.find(Point3::from((0.50, 0.30, 0.300)))
                .expect("Point is a valid point in mesh"),
            TmeshDirection::Right,
            0.1,
        )
        .expect("Local knot insertion should succeed");

    let p3_prime = *ins_point.read().point();

    let p4_prime = *ins_point
        .read()
        .connected_point(TmeshDirection::Right)
        .read()
        .point();

    let p2_prime = *ins_point
        .read()
        .connected_point(TmeshDirection::Left)
        .read()
        .point();

    // Values verified via https://www.desmos.com/3d/pitkyckhfn
    assert!(
        points_eq(p3_prime, Point3::from((0.5916666, 0.245, 0.41916666))),
        "Inserted point does not match expected cartesian coordinates"
    );
    assert!(
        points_eq(p4_prime, Point3::from((0.75416666, 0.1516666, 0.617916666))),
        "Point right of inserted point does not match expected cartesian coordinates"
    );
    assert!(
        points_eq(p2_prime, Point3::from((0.425, 0.24, 0.3225))),
        "Point left of inserted point does not match expected cartesian coordinates"
    );

    for s in 0..N {
        let s = s as f64 / N as f64;
        for t in 0..N {
            let t = t as f64 / N as f64;
            let mesh_sub = mesh.subs(s, t).expect("Parametric point is within bounds");
            let test_sub = test.subs(s, t).expect("Parametric point is within bounds");
            assert!(
                (mesh_sub - test_sub).so_small(),
                "Surfaces do not match at ({}, {}).",
                s,
                t
            );
        }
    }
}

/// Test illegal local knot insertion by creating two identical surfaces, then performing LKI on one of
/// them and checking if an error is returned. Initially, this test ould have succeeded (at leats, LKI would have),
/// however, it was discovered that the surface would change shape if done with one of the control points missing
/// (substituted with an edge condition). Thus, that change was reverted.
#[test]
fn test_t_mesh_local_knot_insertion_edge_conditions() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 1.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 1.0)),
    ];

    let mut mesh = Tmesh::new(points, 1.0);

    // Make mesh a 5x5
    let _ = mesh.subdivide(average_points);
    let _ = mesh.subdivide(average_points);

    println!("{}", mesh);

    let mut test = mesh.clone();
    let ins_point = test.try_local_knot_insertion(
        test.find(Point3::from((1.0, 1.0, 0.0)))
            .expect("Point is a valid point in mesh"),
        TmeshDirection::Down,
        0.1,
    );

    assert!(
        ins_point.is_err(),
        "Local knot insertion should not have succeedd"
    );
}

#[test]
fn test_t_mesh_absolute_local_knot_insertion_mesh_construction() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 0.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 0.0)),
    ];

    // 5x5
    let mut mesh = Tmesh::new(points, 1.0);
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");

    // Insert vertical aspect of the plus
    mesh.try_absolute_local_knot_insertion((0.52, 0.00))
        .expect("Legal point insertion");
    mesh.try_absolute_local_knot_insertion((0.52, 0.25))
        .expect("Legal point insertion");
    mesh.try_absolute_local_knot_insertion((0.52, 0.50))
        .expect("Legal point insertion");
    mesh.try_absolute_local_knot_insertion((0.52, 0.75))
        .expect("Legal point insertion");
    mesh.try_absolute_local_knot_insertion((0.52, 1.00))
        .expect("Legal point insertion");

    // Insert horizontal aspect of the plus
    mesh.try_absolute_local_knot_insertion((0.50, 0.52))
        .expect("Legal point insertion");
    mesh.try_absolute_local_knot_insertion((0.75, 0.52))
        .expect("Legal point insertion");

    // Insert center point of the plus
    let center_point = mesh
        .try_absolute_local_knot_insertion((0.52, 0.52))
        .expect("Legal point insertion");

    // Test absolute knot coordinates of the center point.
    let knot_coords = center_point.read().knot_coordinates();
    assert!(
        (knot_coords.0 + knot_coords.1 - 0.52 - 0.52).so_small(),
        "Knot coordinates for center point do not match expectation."
    );

    // At this point, there is little reason to check if the knot intervals match the expectation, since the
    // center point insertion would have failed, or one of the assertions below would have failed because the
    // LKI is highly sensitive to knot intervals, thus, errors in the algorithm would either lead to a failure
    // in future insertions, a mismatch in absolut knot coordinates, or a missing point connection (or two).
    for dir in TmeshDirection::iter() {
        assert_eq!(
            center_point.read().con_type(dir),
            TmeshConnectionType::Point,
            "Center point is not connected to a point in the direction {}.",
            dir
        );
    }
}

/// Constructs the following T-mesh, insreting a point which would require the use of the ray-casting algorithm to
/// calculate on of the knot intervals used in the calculation of the new cartesian coordinates of the control
/// points affected by LKI. A comparison much like the other LKI insertion tests is done, where two identical meshes
///  are constructed, and then compared using `subs` after one has been modified through the use of LKI. Though a T-junction
/// technically exists in the LKI, none of the LKI rules are broken, since the four required control points still exist,
/// and their perpendicular knot vectors are all equal.
///
/// Uses absolute local knot insertion.
///
/// ```
///        |   |   |   |   |   |
/// 1.00 --+---+--[+]--+---+---+--
///        |   |   |   |   |   |
/// 0.75 --+---+--[+]--+---+---+--
///        |   |   |   |   |   |
/// 0.52   |   +--<+>--+   |   |
///        |   |   |   |   |   |
/// 0.50 --+---+--{+}--+---+---+--
///        |   |   |   |   |   |
/// 0.25 --+---+--[+]--+---+---+--
///        |   |       |   |   |
/// 0.00 --+---+-------+---+---+--
///        |   |       |   |   |
///        0.00|   0.27|   0.75|
///            0.25    0.50    1.00
/// ```
///
/// - `[+]` are control points which are required by LKI
/// - `{+}` is the control point from which the algorithm will insert the new control point using the `try_local_knot_insertion` function.
/// - `<+>` is the point that is inserted in one mesh and not the other.
#[test]
fn test_t_mesh_local_knot_insertion_force_ray_casting() {
    const N: usize = 25;
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 1.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 1.0)),
    ];

    // 5x5
    let mut mesh = Tmesh::new(points, 1.0);
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");

    // Mangle linearity of control points in cartesian space
    mesh.map_point(
        Point3::from((0.25, 0.25, 0.375)),
        Point3::from((0.10, 0.25, 0.375)),
    )
    .expect("Control point is in mesh");
    mesh.map_point(
        Point3::from((0.25, 0.50, 0.500)),
        Point3::from((0.30, 0.50, 0.300)),
    )
    .expect("Control point is in mesh");
    mesh.map_point(
        Point3::from((0.25, 0.75, 0.625)),
        Point3::from((0.15, 0.75, 0.625)),
    )
    .expect("Control point is in mesh");
    mesh.map_point(
        Point3::from((0.25, 1.00, 0.750)),
        Point3::from((0.25, 1.00, 0.200)),
    )
    .expect("Control point is in mesh");

    // Insert vertical aspect of the plus
    mesh.try_absolute_local_knot_insertion((0.27, 0.25))
        .expect("Legal point insertion");
    mesh.try_absolute_local_knot_insertion((0.27, 0.50))
        .expect("Legal point insertion");
    mesh.try_absolute_local_knot_insertion((0.27, 0.75))
        .expect("Legal point insertion");
    mesh.try_absolute_local_knot_insertion((0.27, 1.00))
        .expect("Legal point insertion");

    // Insert horizontal aspect of the plus
    mesh.try_absolute_local_knot_insertion((0.25, 0.52))
        .expect("Legal point insertion");
    mesh.try_absolute_local_knot_insertion((0.50, 0.52))
        .expect("Legal point insertion");

    let mut test = mesh.clone();
    test.try_absolute_local_knot_insertion((0.27, 0.52))
        .expect("Legal point insertion");

    for s in 0..N {
        let s = s as f64 / N as f64;
        for t in 0..N {
            let t = t as f64 / N as f64;
            let mesh_sub = mesh.subs(s, t).expect("Parametric point is within bounds");
            let test_sub = test.subs(s, t).expect("Parametric point is within bounds");
            assert!(
                (mesh_sub - test_sub).so_small(),
                "Surfaces do not match at ({}, {}).",
                s,
                t
            );
        }
    }
}

/// Tests that `to_bspline_surface` produces a B-spline that closely approximates the original T-mesh.
#[test]
fn test_to_bspline_surface_accuracy() {
    use monstertruck_core::cgmath64::*;

    let points = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 0.0, 1.0),
        Point3::new(1.0, 1.0, 2.0),
        Point3::new(0.0, 1.0, 1.0),
    ];
    let mut mesh = Tmesh::new(points, 1.0);
    mesh.subdivide(average_points)
        .expect("Subdivision should succeed");
    mesh.subdivide(average_points)
        .expect("Subdivision should succeed");

    let bsp = mesh.to_bspline_surface(8);

    // Sample both surfaces on an interior grid and check max deviation.
    let n = 20;
    let mut max_err = 0.0f64;
    for i in 1..n {
        let u = i as f64 / n as f64;
        for j in 1..n {
            let v = j as f64 / n as f64;
            let p_tmesh = ParametricSurface::subs(&mesh, u, v);
            let p_bsp = ParametricSurface::subs(&bsp, u, v);
            let err = (p_tmesh - p_bsp).magnitude();
            max_err = max_err.max(err);
        }
    }
    assert!(
        max_err < 1.0e-3,
        "Max deviation between T-mesh and B-spline: {max_err:.2e} (expected < 1e-3)."
    );
}

/// Constructs a non-planar T-mesh for derivative testing.
/// Uses the same pattern as `test_t_mesh_local_knot_insertion_no_edge_conditions`.
fn make_derivative_test_mesh() -> Tmesh<Point3> {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 1.0)),
        Point3::from((1.0, 1.0, 0.0)),
        Point3::from((0.0, 1.0, 1.0)),
    ];
    let mut mesh = Tmesh::new(points, 1.0);
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");

    // Mangle linearity so derivatives are nontrivial.
    mesh.map_point(
        Point3::from((0.25, 0.25, 0.375)),
        Point3::from((0.25, 0.10, 0.375)),
    )
    .expect("Control point is in mesh");
    mesh.map_point(
        Point3::from((0.50, 0.25, 0.500)),
        Point3::from((0.50, 0.30, 0.300)),
    )
    .expect("Control point is in mesh");
    mesh.map_point(
        Point3::from((0.75, 0.25, 0.625)),
        Point3::from((0.75, 0.15, 0.625)),
    )
    .expect("Control point is in mesh");
    mesh
}

/// Computes a finite-difference derivative for comparison with analytical derivatives.
fn finite_diff_der(mesh: &Tmesh<Point3>, m: usize, n: usize, u: f64, v: f64) -> Vector3 {
    let h = 1.0e-6;
    if m == 0 && n == 0 {
        let p = mesh.subs(u, v).expect("subs should succeed");
        return Vector3::new(p.x, p.y, p.z);
    }
    if m > 0 {
        let fwd = finite_diff_der(mesh, m - 1, n, u + h, v);
        let bwd = finite_diff_der(mesh, m - 1, n, u - h, v);
        (fwd - bwd) / (2.0 * h)
    } else {
        let fwd = finite_diff_der(mesh, m, n - 1, u, v + h);
        let bwd = finite_diff_der(mesh, m, n - 1, u, v - h);
        (fwd - bwd) / (2.0 * h)
    }
}

/// Tests analytical derivatives against finite differences at many sample points.
#[test]
fn test_analytical_derivatives_vs_finite_diff() {
    let mesh = make_derivative_test_mesh();
    let n = 10;

    for i in 1..n {
        let u = i as f64 / n as f64;
        for j in 1..n {
            let v = j as f64 / n as f64;
            // Test all 1st and 2nd order derivatives.
            // 2nd-order finite differences are inherently less accurate (O(h^2) error
            // compounds), so use a looser tolerance for them.
            for &(m, ord_n, tol) in &[
                (1, 0, 1.0e-4),
                (0, 1, 1.0e-4),
                (2, 0, 5.0e-4),
                (0, 2, 5.0e-4),
                (1, 1, 5.0e-4),
            ] {
                let analytical = mesh.analytical_der_mn(m, ord_n, u, v);
                let numerical = finite_diff_der(&mesh, m, ord_n, u, v);
                let diff = (analytical - numerical).magnitude();
                assert!(
                    diff < tol,
                    "Derivative d^({},{}) at ({}, {}) differs: analytical={:?}, numerical={:?}, diff={:.2e}",
                    m,
                    ord_n,
                    u,
                    v,
                    analytical,
                    numerical,
                    diff
                );
            }
        }
    }
}

/// Tests derivative continuity at knot boundaries.
#[test]
fn test_analytical_derivatives_knot_continuity() {
    let mesh = make_derivative_test_mesh();
    let eps = 1.0e-8;
    let tol = 1.0e-4;

    // Knot boundaries for a 5x5 mesh are at 0.0, 0.25, 0.5, 0.75, 1.0.
    let knots = [0.25, 0.5, 0.75];
    for &k in &knots {
        for &(m, n) in &[(1, 0), (0, 1)] {
            // Test continuity across u-knot boundary.
            let left = mesh.analytical_der_mn(m, n, k - eps, 0.5);
            let right = mesh.analytical_der_mn(m, n, k + eps, 0.5);
            let diff = (left - right).magnitude();
            assert!(
                diff < tol,
                "Derivative d^({},{}) discontinuous at u-knot {}: left={:?}, right={:?}, diff={:.2e}",
                m,
                n,
                k,
                left,
                right,
                diff
            );

            // Test continuity across v-knot boundary.
            let below = mesh.analytical_der_mn(m, n, 0.5, k - eps);
            let above = mesh.analytical_der_mn(m, n, 0.5, k + eps);
            let diff = (below - above).magnitude();
            assert!(
                diff < tol,
                "Derivative d^({},{}) discontinuous at v-knot {}: below={:?}, above={:?}, diff={:.2e}",
                m,
                n,
                k,
                below,
                above,
                diff
            );
        }
    }
}

/// Tests derivatives at parametric boundary values.
#[test]
fn test_analytical_derivatives_at_boundaries() {
    let mesh = make_derivative_test_mesh();
    let tol = 1.0e-4;
    let eps = 1.0e-7;

    // Test at near-boundary points (exact boundary 1.0 is excluded from basis support).
    for &(u, v) in &[(eps, 0.5), (0.5, eps), (1.0 - eps, 0.5), (0.5, 1.0 - eps)] {
        for &(m, n) in &[(1, 0), (0, 1)] {
            let analytical = mesh.analytical_der_mn(m, n, u, v);
            let numerical = finite_diff_der(&mesh, m, n, u, v);
            let diff = (analytical - numerical).magnitude();
            assert!(
                diff < tol,
                "Boundary derivative d^({},{}) at ({}, {}) differs: diff={:.2e}",
                m,
                n,
                u,
                v,
                diff
            );
        }
    }
}

/// Tests `refine_at` at a location where an edge already exists -- should behave identically
/// to `try_absolute_local_knot_insertion`.
#[test]
fn test_refine_at_existing_edge() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 1.0)),
        Point3::from((1.0, 1.0, 2.0)),
        Point3::from((0.0, 1.0, 1.0)),
    ];
    let mut mesh = Tmesh::new(points, 1.0);
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");

    let mut reference = mesh.clone();

    // Direct LKI should succeed at this location (on an existing edge).
    reference
        .try_absolute_local_knot_insertion((0.3, 0.25))
        .expect("Direct LKI should succeed");

    // refine_at should produce the same result.
    mesh.refine_at(0.3, 0.25).expect("refine_at should succeed");

    // Verify surfaces match.
    let n = 20;
    for i in 0..n {
        let u = i as f64 / n as f64;
        for j in 0..n {
            let v = j as f64 / n as f64;
            let a = mesh.subs(u, v).expect("subs should succeed");
            let b = reference.subs(u, v).expect("subs should succeed");
            assert!(
                (a - b).so_small(),
                "Surfaces differ at ({}, {}): refine_at={:?}, direct={:?}",
                u,
                v,
                a,
                b
            );
        }
    }
}

/// Tests `refine_at` at a location requiring an intermediate edge insertion.
#[test]
fn test_refine_at_with_intermediate_edge() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 1.0)),
        Point3::from((1.0, 1.0, 2.0)),
        Point3::from((0.0, 1.0, 1.0)),
    ];
    let mut mesh = Tmesh::new(points, 1.0);
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");

    let original = mesh.clone();

    // (0.3, 0.3) has no straddling edge -- it's in the interior of a face.
    // Direct LKI would fail.
    assert!(
        mesh.clone()
            .try_absolute_local_knot_insertion((0.3, 0.3))
            .is_err(),
        "Direct LKI should fail at (0.3, 0.3) -- no straddling edge."
    );

    // refine_at should succeed by inserting intermediate edges.
    mesh.refine_at(0.3, 0.3)
        .expect("refine_at should succeed with intermediate edges");

    // Verify surface is unchanged (shape-preserving).
    let n = 20;
    for i in 0..n {
        let u = i as f64 / n as f64;
        for j in 0..n {
            let v = j as f64 / n as f64;
            let a = mesh.subs(u, v).expect("subs should succeed");
            let b = original.subs(u, v).expect("subs should succeed");
            assert!(
                (a - b).so_small(),
                "Surface changed after refine_at at ({}, {}): refined={:?}, original={:?}",
                u,
                v,
                a,
                b
            );
        }
    }
}

/// Tests that `refine_at` preserves the surface shape.
#[test]
fn test_refine_at_shape_preserving() {
    let points = [
        Point3::from((0.0, 0.0, 0.0)),
        Point3::from((1.0, 0.0, 1.0)),
        Point3::from((1.0, 1.0, 2.0)),
        Point3::from((0.0, 1.0, 1.0)),
    ];
    let mut mesh = Tmesh::new(points, 1.0);
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");
    mesh.subdivide(average_points)
        .expect("Mesh is not malformed.");

    let original = mesh.clone();

    // Insert on an existing horizontal edge at t=0.
    mesh.refine_at(0.6, 0.0)
        .expect("refine_at on edge should succeed");

    // Insert requiring intermediate edges.
    mesh.refine_at(0.6, 0.5)
        .expect("refine_at with intermediate should succeed");

    // Verify surface is unchanged.
    let n = 20;
    let mut max_err = 0.0f64;
    for i in 0..n {
        let u = i as f64 / n as f64;
        for j in 0..n {
            let v = j as f64 / n as f64;
            let a = mesh.subs(u, v).expect("subs should succeed");
            let b = original.subs(u, v).expect("subs should succeed");
            let err = (a - b).magnitude();
            max_err = max_err.max(err);
        }
    }
    assert!(
        max_err < 1.0e-10,
        "Surface shape changed after refinement, max error: {max_err:.2e}"
    );
}

/// Round-trip test: BsplineSurface → Tmesh → evaluate, compare with original.
#[test]
fn test_from_bspline_surface_round_trip() {
    use monstertruck_core::cgmath64::*;

    let u_knots = KnotVector::uniform_knot(3, 3);
    let v_knots = KnotVector::uniform_knot(3, 3);
    // 3 (degree) + 3 (divisions).
    let nu = 6;
    let nv = 6;

    // Build a non-planar cubic B-spline surface.
    let cps: Vec<Vec<Point3>> = (0..nu)
        .map(|i| {
            (0..nv)
                .map(|j| {
                    let x = i as f64 / (nu - 1) as f64;
                    let y = j as f64 / (nv - 1) as f64;
                    let z = (x * std::f64::consts::PI).sin() * (y * std::f64::consts::PI).sin();
                    Point3::new(x, y, z)
                })
                .collect()
        })
        .collect();
    let bsp = BsplineSurface::new((u_knots, v_knots), cps);

    let tmesh = Tmesh::from_bspline_surface(&bsp).expect("Conversion should succeed");

    // Sample both surfaces and compare.
    let n = 15;
    let mut max_err = 0.0f64;
    for i in 1..n {
        let u = i as f64 / n as f64;
        for j in 1..n {
            let v = j as f64 / n as f64;
            let p_bsp = ParametricSurface::subs(&bsp, u, v);
            let p_tmesh = tmesh.subs(u, v).expect("T-mesh eval should succeed");
            let err = (p_bsp - p_tmesh).magnitude();
            max_err = max_err.max(err);
        }
    }
    assert!(
        max_err < 1.0e-6,
        "Bspline → Tmesh round-trip max error: {max_err:.2e} (expected < 1e-6)"
    );
}

/// Verifies correct structure: control point count and interior connectivity.
#[test]
fn test_from_bspline_surface_structure() {
    let u_knots = KnotVector::uniform_knot(3, 2);
    let v_knots = KnotVector::uniform_knot(3, 3);
    let nu = 5;
    let nv = 6;

    let cps: Vec<Vec<Point3>> = (0..nu)
        .map(|i| {
            (0..nv)
                .map(|j| Point3::new(i as f64, j as f64, 0.0))
                .collect()
        })
        .collect();
    let bsp = BsplineSurface::new((u_knots, v_knots), cps);

    let tmesh = Tmesh::from_bspline_surface(&bsp).expect("Conversion should succeed");

    // Correct number of control points.
    assert_eq!(tmesh.control_points().len(), nu * nv);

    // Interior points should have 4 point connections.
    let mut interior_count = 0;
    for cp in tmesh.control_points() {
        let r = cp.read();
        let point_connections: usize = TmeshDirection::iter()
            .filter(|&d| r.con_type(d) == TmeshConnectionType::Point)
            .count();
        if point_connections == 4 {
            interior_count += 1;
        }
    }
    assert_eq!(
        interior_count,
        (nu - 2) * (nv - 2),
        "Expected {} interior points with 4 connections, got {}",
        (nu - 2) * (nv - 2),
        interior_count
    );
}

/// Non-cubic B-spline surfaces should be rejected.
#[test]
fn test_from_bspline_surface_non_cubic() {
    // Degree 2 in u, degree 2 in v.
    let u_knots = KnotVector::uniform_knot(2, 2);
    let v_knots = KnotVector::uniform_knot(2, 2);
    let nu = 4;
    let nv = 4;

    let cps: Vec<Vec<Point3>> = (0..nu)
        .map(|i| {
            (0..nv)
                .map(|j| Point3::new(i as f64, j as f64, 0.0))
                .collect()
        })
        .collect();
    let bsp = BsplineSurface::new((u_knots, v_knots), cps);

    let result = Tmesh::from_bspline_surface(&bsp);
    assert!(result.is_err(), "Non-cubic surface should be rejected");
}

/// Flat surface should have zero Gaussian curvature → zero insertions.
#[test]
fn test_adaptive_refine_flat_surface() {
    let points = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(1.0, 1.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
    ];
    let mut mesh = Tmesh::new(points, 1.0);
    mesh.subdivide(average_points).expect("Subdivision ok");
    mesh.subdivide(average_points).expect("Subdivision ok");

    let insertions = mesh
        .adaptive_refine(0.01, 3, 5)
        .expect("Adaptive refine should succeed");
    assert_eq!(
        insertions, 0,
        "Flat surface should have 0 insertions, got {insertions}"
    );
}

/// Curved surface should get insertions, and the surface shape should be preserved.
#[test]
fn test_adaptive_refine_curved_surface() {
    // Use a saddle shape which has intrinsic curvature.
    let points = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 0.0, 1.0),
        Point3::new(1.0, 1.0, 0.0),
        Point3::new(0.0, 1.0, 1.0),
    ];
    let mut mesh = Tmesh::new(points, 1.0);
    mesh.subdivide(average_points).expect("ok");
    mesh.subdivide(average_points).expect("ok");
    let original = mesh.clone();

    let insertions = mesh
        .adaptive_refine(0.1, 2, 5)
        .expect("Adaptive refine should succeed");
    assert!(
        insertions > 0,
        "Curved surface should have insertions, got 0"
    );

    // Verify shape preservation.
    let n = 15;
    let mut max_err = 0.0f64;
    for i in 0..n {
        let u = i as f64 / n as f64;
        for j in 0..n {
            let v = j as f64 / n as f64;
            let a = mesh.subs(u, v).expect("subs ok");
            let b = original.subs(u, v).expect("subs ok");
            let err = (a - b).magnitude();
            max_err = max_err.max(err);
        }
    }
    assert!(
        max_err < 1.0e-10,
        "Surface changed after adaptive refinement, max error: {max_err:.2e}"
    );
}

/// Gaussian curvature should be nonzero at curved regions and ~zero at flat ones.
#[test]
fn test_gaussian_curvature_sanity() {
    // Build a flat mesh.
    let flat_points = [
        Point3::new(0.0, 0.0, 0.0),
        Point3::new(1.0, 0.0, 0.0),
        Point3::new(1.0, 1.0, 0.0),
        Point3::new(0.0, 1.0, 0.0),
    ];
    let mut flat = Tmesh::new(flat_points, 1.0);
    flat.subdivide(average_points).expect("ok");
    flat.subdivide(average_points).expect("ok");

    let k_flat = flat.gaussian_curvature(0.5, 0.5);
    assert!(
        k_flat.abs() < 1.0e-6,
        "Flat surface curvature should be ~0, got {k_flat:.2e}"
    );

    // Build a curved mesh (saddle shape with mangled points).
    let curved = make_derivative_test_mesh();
    let k_curved = curved.gaussian_curvature(0.3, 0.3);
    assert!(
        k_curved.abs() > 1.0e-3,
        "Curved surface should have significant curvature, got {k_curved:.2e}"
    );
}
