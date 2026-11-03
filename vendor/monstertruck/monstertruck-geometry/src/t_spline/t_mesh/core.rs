use super::*;

impl<P> Tmesh<P> {
    /// Constructs a new rectangular T-mesh from four points in space and a value for
    /// outward-facing knot intervals. The result is the following mesh, where the
    /// numbers are the indices of the array `points`. The knot interval between
    /// each point is 1.0.
    /// ```text
    ///  3|   |2
    /// --+---+--
    ///   |   |
    /// --+---+--
    ///  0|   |1
    /// ```
    pub fn new(points: [P; 4], edge_knot_interval: f64) -> Tmesh<P> {
        // Convert points into control points
        let control_points: Vec<Arc<RwLock<TmeshControlPoint<P>>>> = Vec::from(points)
            .into_iter()
            .map(|p| {
                let cont_point = TmeshControlPoint::new(p, edge_knot_interval);
                Arc::new(RwLock::new(cont_point))
            })
            .collect();

        // Set the first point as the "knot origin". This may result in some negative components in the
        // knot vectors of the points near the left and bottom edge condition, but this should not matter (test?)
        control_points[0]
            .write()
            .set_knot_coordinates(0.0, 0.0)
            .expect("No connections have been created for the current mesh");

        // Connect control points according to the diagram in the docs
        let mut dir = TmeshDirection::Right;
        for i in 0..4 {
            control_points[i]
                .write()
                .remove_edge_condition(dir)
                .expect("Point edge conditions are known at compile time");

            control_points[(i + 1) % 4]
                .write()
                .remove_edge_condition(dir.flip())
                .expect("Point edge conditions are known at compile time");

            // Connect the point i to the point i plus one
            TmeshControlPoint::connect(
                Arc::clone(&control_points[i % 4]),
                Arc::clone(&control_points[(i + 1) % 4]),
                dir,
                1.0,
            )
            .expect("T-mesh connections are known valid at compile time");

            dir = dir.anti_clockwise();
        }

        Tmesh {
            control_points,
            knot_vectors: RwLock::new(None),
        }
    }

    /// Returns an immutable reference to the control points vector
    pub fn control_points(&self) -> &Vec<Arc<RwLock<TmeshControlPoint<P>>>> { &self.control_points }

    /// Inserts a control point with real space coordinates `p` on the side `connection_side`
    /// of `con`. The knot interval of the connection between con and the new control point
    /// is the current weight of the connection multiplied by the ratio. Thus if ratio is
    /// 0.0, the connection between con and the new control point will have an interval of
    /// 0.0. `con` must be a control point in `self` and the new control point `p` must be
    /// inserted between two existing points, that is, `con`'s connection on the side
    /// `connection_side` must not be an edge condition or a T-junction.
    ///
    /// >NOTE!
    /// > This will change the shape of the resulting surface.
    /// > Use Local Knot Insertion in order to add a control point
    /// > without changing the shape of the surface.
    ///
    /// # Returns
    /// - `TmeshInvalidKnotRatio` if `knot_ratio` is not in \[0.0, 1.0\].
    ///
    /// - `TmeshConnectionNotFound` if `con` has no connection on `connection_side`.
    ///
    /// - `TmeshControlPointNotFound` if `con` is an edge condition on `connction_side`.
    ///
    /// - `TmeshForeignControlPoint` if `con` is not a control point in the T-mesh.
    ///
    /// - `TmeshConnectionInvalidKnotInterval` if the connection between `con`
    ///   and the point in the direction `connection_side`, `con_side`, does not have the same
    ///   knot interval in both directions (`con` -> `con_side` != `con` <- `con_side`).
    ///   This should never happen.
    ///
    /// - `Ok(Arc<RwLock<TmeshControlPoint<P>>>)` if the control point was successfully added, which itself is returned.
    ///
    /// # Borrows
    /// Mutably borrows `con` and the point located in the direction `connection_side`, and potentially borrows all
    /// points that are a part of the faces on either side of the edge that connects `p` and the point located in
    /// the direction `connection_side`.
    ///
    /// # Panics
    /// Panics if any borrow does not succeed.
    pub fn add_control_point(
        &mut self,
        p: P,
        con: Arc<RwLock<TmeshControlPoint<P>>>,
        connection_side: TmeshDirection,
        knot_ratio: f64,
    ) -> Result<Arc<RwLock<TmeshControlPoint<P>>>> {
        // Check that the knot ratio is valid
        if !(0.0..=1.0).contains(&knot_ratio) {
            return Err(Error::TmeshInvalidKnotRatio);
        }

        // If con is not found in the mesh, return the corresponding error.
        if self
            .control_points
            .iter()
            .position(|x| Arc::ptr_eq(x, &con))
            .is_none()
        {
            return Err(Error::TmeshForeignControlPoint);
        }

        // Get the point currently connected to the connection point. Returns the
        // requisit errors in the case that the connection is not of type Point.
        let other_point = {
            let borrow = con.read();
            Arc::clone(&borrow.try_connected_point(connection_side)?)
        };

        // Edge weights for p are set to 0.0, however, the final step will overwrite this
        // if a different edge weight was specified in the T-mesh constructor
        let p = Arc::new(RwLock::new(TmeshControlPoint::new(p, 0.0)));

        let knot_interval = con
            .read()
            .connection_knot(connection_side)
            .ok_or(Error::TmeshConnectionNotFound)?;

        let other_knot_interval = other_point
            .read()
            .connection_knot(connection_side.flip())
            .ok_or(Error::TmeshConnectionNotFound)?;

        // Confirm that the knot intervals are the same in both directions.
        if !(knot_interval - other_knot_interval).so_small() {
            return Err(Error::TmeshConnectionInvalidKnotInterval);
        }

        // Break connections between con_point and other_point
        con.write()
            .remove_connection(connection_side)
            .expect("Guaranteed by previous checks");

        // Remove edge conditions for p
        p.write()
            .remove_edge_condition(connection_side)
            .expect("New control point has known edge conditions");
        p.write()
            .remove_edge_condition(connection_side.flip())
            .expect("New control point has known edge conditions");

        // Insert p with the proper knot intervals.
        // con <-> other becomes con <-> p <-> other
        // con <-> p
        TmeshControlPoint::connect(
            Arc::clone(&con),
            Arc::clone(&p),
            connection_side,
            knot_interval * knot_ratio,
        )
        .map_err(|_| Error::TmeshUnknownError)?;

        // p <-> other
        TmeshControlPoint::connect(
            Arc::clone(&p),
            Arc::clone(&other_point),
            connection_side,
            knot_interval * (1.0 - knot_ratio),
        )
        .map_err(|_| Error::TmeshUnknownError)?;

        // When a new point is added, there can only possibly be edge conditions on
        // the two sides perpendicular to the connection. If there is no edge condition,
        // Rule 2 for T-meshes [Sederberg et al. 2003] should be checked to find any
        // inferred connections (ic), and if it does not apply, the connection is removed.

        // TODO: Currently this code does not allow for knot intervals of 0, and needs to be
        // updated once a solution to figure 9 in [Sederberg et al. 2003] is found.
        if con.read().con_type(connection_side.clockwise()) == TmeshConnectionType::Edge {
            let edge_weight = con
                .read()
                .connection_knot(connection_side.clockwise())
                .expect("Edges must have a weight");

            p.write()
                .set_edge_con_weight(connection_side.clockwise(), edge_weight)
                .expect("New points have edge conditions as default connection type.");
        } else {
            // Remove the edge condition created by the constructor.
            let _ = p.write().remove_edge_condition(connection_side.clockwise());

            // If a point that satisfies Rule 2 from [Sederberg et al. 2003] is found, connect it.
            // Should also never return an error.
            self.find_inferred_connection(Arc::clone(&p), connection_side.clockwise())
                .map_err(|_| Error::TmeshUnknownError)?;
        }

        if con.read().con_type(connection_side.anti_clockwise()) == TmeshConnectionType::Edge {
            let edge_weight = con
                .read()
                .connection_knot(connection_side.anti_clockwise())
                .expect("Edges must have a weight");

            p.write()
                .set_edge_con_weight(connection_side.anti_clockwise(), edge_weight)
                .expect("New points have edge conditions as default connection type.");
        } else {
            // Remove the edge condition created by the constructor.
            let _ = p
                .write()
                .remove_edge_condition(connection_side.anti_clockwise());

            // If a point that satisfies Rule 2 from [Sederberg et al. 2003] is found, connect it.
            // Should also never return an error.
            self.find_inferred_connection(Arc::clone(&p), connection_side.anti_clockwise())
                .map_err(|_| Error::TmeshUnknownError)?;
        }

        // Add control point
        self.control_points.push(Arc::clone(&p));
        *self.knot_vectors.write() = None;
        Ok(p)
    }

    /// Attemps to add a control point to the mesh given the cartesian point `p` and the absolute knot coordinates `knot_coords`
    /// in the form `(s, t)`. In order for insertion to succeed, there must either be an S or T edge located at the parametric
    /// point `knot_coords` in the mesh `self`. Note that zero knot insertions will return an error, as the side on which to
    /// insert the zero knot is ambiguous.
    ///
    /// # Returns
    /// - `TmeshOutOfBoundsInsertion` if a control point is being inserted with either knot coordinate out of the range `[0.0, 1.0]`.
    ///
    /// - `TmeshExistingControlPoint` if a control point already exists at parametric coordinates `knot_coords`.
    ///
    /// - `TmeshMalformedMesh` if multiple edges are found which intersect the location of the new point.
    ///
    /// - `TmeshConnectionNotFound` if no edges are found which intersect the location of the new point.
    ///
    /// - `Ok(Arc<RwLock<TmeshControlPoint<P>>>)` if the control point was successfully added, which itself is returned.
    ///
    /// # Borrows
    /// Immutably borrows every point in the mesh `self`.
    pub fn try_add_absolute_point(
        &mut self,
        p: P,
        knot_coords: (f64, f64),
    ) -> Result<Arc<RwLock<TmeshControlPoint<P>>>> {
        // Make sure desred knot coordinates are within mesh bounds
        if knot_coords.0 < 0.0 || knot_coords.0 > 1.0 || knot_coords.1 < 0.0 || knot_coords.1 > 1.0
        {
            return Err(Error::TmeshOutOfBoundsInsertion);
        }

        // If a point already exists at the desired knot coordinates, return an error. Zero knot intervals can be put
        // on any side of a point and still have the same knot coordinates, but the structure of the mesh will not be
        // different. Thus, zero knot insertion must be done manually.
        if self
            .control_points
            .iter()
            .find(|c| {
                let c_coords = c.read().knot_coordinates();
                let comparison = (c_coords.0 - knot_coords.0, c_coords.1 - knot_coords.1);
                comparison.0.so_small() && comparison.1.so_small()
            })
            .is_some()
        {
            return Err(Error::TmeshExistingControlPoint);
        }

        // The function checks for any T or S edges that intersect the point in paramtric space where the
        // point is to be insertet, then computes the knot ratio needed such that the point is inserted
        // at the correct place and inserts it using add_control_point.

        // Check for any T edges which intersect the parametric location of the new point.
        let mut point_t_coord = 0.0;
        let mut con_knot = 0.0;
        let s_axis_straddle_points = self
            .control_points
            .iter()
            // Filter all points along the S axis of inserton
            .filter(|point| (point.read().knot_coordinates().0 - knot_coords.0).so_small())
            // Filter those points to only include the point that straddles the T axis of insertion
            .filter(|point| {
                if let Some(con) = point.read().get(TmeshDirection::Up) {
                    let temp_t_coord = point.read().knot_coordinates().1;
                    let temp_inter = con.1;

                    // Knot of the new point is located on the connection being investigated?
                    if temp_t_coord < knot_coords.1 && temp_t_coord + temp_inter > knot_coords.1 {
                        point_t_coord = temp_t_coord; // T coordinate of the current point
                        con_knot = temp_inter; // Edge knot interval

                        return true;
                    }
                }
                false
            })
            .map(Arc::clone)
            .collect::<Vec<Arc<RwLock<TmeshControlPoint<P>>>>>();

        // Depending on the number of points whose connections intersect the location of the new point,
        // different errors or actions are taken
        match s_axis_straddle_points.len() {
            // No T-edge instersects the point where the point needs to be inserted,
            // try to find an S edge which intersects the location of the point
            0 => {}
            1 => {
                // A T-edge is found where the point intersects
                return self
                    .add_control_point(
                        p,
                        Arc::clone(&s_axis_straddle_points[0]),
                        TmeshDirection::Up,
                        (knot_coords.1 - point_t_coord) / con_knot,
                    )
                    .map_err(|_| Error::TmeshUnknownError);
            }
            _ => {
                // Multiple T-edges are found where the point intersects (Should never happen)
                return Err(Error::TmeshMalformedMesh);
            }
        };

        let mut point_s_coord = 0.0;
        let mut con_knot = 0.0;
        let t_axis_straddle_points = self
            .control_points
            .iter()
            // Filter all points along the T axis of inserton
            .filter(|point| (point.read().knot_coordinates().1 - knot_coords.1).so_small())
            // Filter those points to only include the point that straddles the S axis of insertion
            .filter(|point| {
                if let Some(con) = point.read().get(TmeshDirection::Right) {
                    let temp_s_coord = point.read().knot_coordinates().0;
                    let temp_inter = con.1;

                    // Knot of the new point is located on the connection being investigated?
                    if temp_s_coord < knot_coords.0 && temp_s_coord + temp_inter > knot_coords.0 {
                        point_s_coord = temp_s_coord; // S coordinate of the current point
                        con_knot = temp_inter; // Edge knot interval

                        return true;
                    }
                }
                false
            })
            .map(Arc::clone)
            .collect::<Vec<Arc<RwLock<TmeshControlPoint<P>>>>>();

        // Depending on the number of points whose connections intersect the location of the new point,
        // different errors or actions are taken
        match t_axis_straddle_points.len() {
            0 => {
                // No S-edge instersects the point where the point needs to be inserted, return an error
                Err(Error::TmeshConnectionNotFound)
            }
            1 => {
                // An S-edge is found where the point intersects
                self.add_control_point(
                    p,
                    Arc::clone(&t_axis_straddle_points[0]),
                    TmeshDirection::Right,
                    (knot_coords.0 - point_s_coord) / con_knot,
                )
                .map_err(|_| Error::TmeshUnknownError)
            }
            _ => {
                // Multiple S-edges are found where the point intersects (Should never happen)
                Err(Error::TmeshMalformedMesh)
            }
        }
    }

    /// Generates the S and T knot vectors for a particular point. The returned tuple is of the form `(S_vector, T_vector)`,
    /// where `S_vector` is the horizontal knot vector and `T_vector` is the vertical knot vector. Both knot vectors shall
    /// be of length 5
    ///
    /// # Returns
    /// - `TmeshConnectionNotFound` if a T-junction is unexpectedly found (non-rectangular face)
    ///
    /// - `TmeshControlPointNotFound` if an edge condition is unexpectedly found (internal edge condition)
    ///
    /// - `Ok((KnotVector, KnotVector))` if knot vectors are successfully generated
    ///
    /// # Borrows
    /// Immutably borrows `p` and all points connected to `p` in all directions for a distance of two knot intervals.
    /// In the case that `p` is not connected to a point in a direction, but instead a T-junction, any points
    /// that are a part of the face which `p` is a part of and the next face in that direction may be borrowed,
    /// with no guarantees as to which or how many.
    pub(super) fn point_knot_vectors(
        p: Arc<RwLock<TmeshControlPoint<P>>>,
    ) -> Result<(KnotVector, KnotVector)> {
        let mut s_vec: Vec<f64> = vec![0.0; 5];
        let mut t_vec: Vec<f64> = vec![0.0; 5];

        // Center of the knot vec is the knot coordinate of the current point
        s_vec[2] = p.read().knot_coordinates().0;
        t_vec[2] = p.read().knot_coordinates().1;

        // Cast rays in all directions
        for dir in TmeshDirection::iter() {
            let cur_point = Arc::clone(&p);
            // Knot intervals for intersections (These are deltas, not absolutes)
            let knot_intervals = Tmesh::cast_ray(cur_point, dir, 2)?;

            for i in 0..2 {
                let inter = knot_intervals[i];

                // Knot vectors for a point go left to right and lower to upper as the index increases.
                // Knot interval will be the knot interval from the center point to the i'th point in the direction dir.
                // (The mesh will most likely look different, with T junctions and edge conditions)
                //           [T]    Initial cur_point
                //            + 4  /
                //            |   /
                //            + 3/
                //            | /
                //  +----+----+----+----+  [S]
                //  0    1    |    3    4
                //            + 1
                //            |
                //            + 0
                match dir {
                    TmeshDirection::Up => {
                        t_vec[3 + i] = t_vec[2 + i] + inter;
                    }
                    TmeshDirection::Right => {
                        s_vec[3 + i] = s_vec[2 + i] + inter;
                    }
                    TmeshDirection::Down => {
                        t_vec[1 - i] = t_vec[2 - i] - inter;
                    }
                    TmeshDirection::Left => {
                        s_vec[1 - i] = s_vec[2 - i] - inter;
                    }
                }
            }
        }
        Ok((KnotVector::from(s_vec), KnotVector::from(t_vec)))
    }

    /// Generates the knot vectors for each control point using the method in \[Sederberg et al. 2003\].
    /// The knot vector for a control point is located at the same index as the control point is in `self.control_points`.
    /// Each pair of knot vectors is arranged as `(s, t)` where `s` is the horizontal and `t` is the vertical.
    ///
    /// # Returns
    /// All errors returned from the function result from a malformed T-mesh and should not
    /// - `TmeshConnectionNotFound` if a non-rectangular face is encountered.
    ///
    /// - `TmeshControlPointNotFound` if an unexpected edge condition is found.
    ///
    /// - `Ok(())` if knot vectors are successfully generated.
    ///
    /// # Borrows
    /// Immutably borrows every point in `self.control_points`.
    pub(super) fn generate_knot_vectors(&self) -> Result<()> {
        let mut knot_vecs: Vec<(KnotVector, KnotVector)> = Vec::new();

        for control_point in self.control_points.iter() {
            knot_vecs.push(Tmesh::point_knot_vectors(Arc::clone(control_point))?);
        }

        *self.knot_vectors.write() = Some(knot_vecs);
        Ok(())
    }

    /// Finds and creates an inferred connection on the point `p` for the anti-clockwise
    /// face which `face_dir` points into the face and which `p` is a part of. `p` must be part of
    /// a valid face and must not be a corner (a connection cannot already exist in the `face_dir`
    /// direction.)
    ///
    /// > **Warning**\
    /// > Does not check if the face is valid.
    ///
    ///
    /// Example mesh for reference:
    /// ```text
    /// +----+-+-|+|-----{+}
    /// |    |    ^       |
    /// +----+---[+]-+---<+>
    /// |            |    |
    /// +-+----+-----+----+
    /// ```
    /// - `face_dir` points up
    /// - `[+]` is `p`
    /// - `<+>` and `{+}` are used in the internal comments
    ///
    /// `p`, labeled `[+]`, will be connected to `|+|` after calling `find_inferred_connection`
    ///
    /// # Returns
    /// - `TmeshConnectionNotFound` if any connection that is expected to
    ///   exist does not. This should only happen on a malformed T-mesh.
    ///
    /// - `TmeshControlPointNotFound` if any control point that is expected
    ///   to exist does not. This usually happens because the current face does not exist
    ///   (`p` is an edge condition).
    ///
    /// - `TmeshExistingConnection` if a connection exists in the `face_dir` direction
    ///   (`p` is a corner).
    ///
    /// - `Ok(true)` if an inferred connection was found and connected.
    ///
    /// - `Ok(false)` if an inferred connection was not found.
    ///
    /// # Borrows
    /// Immutably borrows all points along the anti-clockwise face path between
    /// `p` and `|+|`.
    ///
    /// Mutably borrows `p` and `|+|`.
    ///
    /// # Panics
    /// If `p` or the potential point to which the inferred connection will go to
    /// cannot be borrowed mutably, `find_inferred_connection` will panic.
    ///
    /// # Zero Knot Intervals
    /// While this function is capable of inserting points with zero knot intervals in every (legaal)
    /// case, there are no guarantees as to how points will be connected with a zero knot interval
    /// regarding implicit connections (Cross-face connections).
    fn find_inferred_connection(
        &mut self,
        p: Arc<RwLock<TmeshControlPoint<P>>>,
        face_dir: TmeshDirection,
    ) -> Result<bool> {
        let mut cur_point = Arc::clone(&p);
        let mut cur_dir = face_dir.clockwise();
        let mut ic_knot_measurement = 0.0; // The distance traversed from p to <+>
        let mut ic_knot_interval = 0.0; // The interval of the ic

        // Check that p is not a corner
        if p.read().con_type(face_dir) == TmeshConnectionType::Point {
            return Err(Error::TmeshExistingConnection);
        }

        // Traverse in the direction cur_dir until an anti-clockwise connection is found.
        // Repeat once to get to the point {+}
        for i in 0..2 {
            let accumulation: f64;

            (cur_point, accumulation) = cur_point
                .read()
                .navigate_until_con(cur_dir, cur_dir.anti_clockwise())?;

            cur_dir = cur_dir.anti_clockwise();

            if i == 0 {
                // Accumulate knot intevals for comparison later. Only accumulate knots that are
                // related to the current face
                ic_knot_measurement = accumulation;
            } else if i == 1 {
                // Accumulate knot intervals for the potential IC knot weight. Only accumulate
                // knots that are related to the current face
                ic_knot_interval = accumulation;
            }
        }

        // After the above loop, cur_point is located at {+} and cur_dir points opposite
        // connection_side. Start accumulating knot intervals until the edge of the face
        // is reached, the accumulation is greater than the measurement, or the two are equal.
        let mut ic_knot_accumulation = 0.0;
        loop {
            ic_knot_accumulation += cur_point
                .read()
                .connection_knot(cur_dir)
                .ok_or(Error::TmeshConnectionNotFound)?;

            cur_point = {
                let borrow = cur_point.read();
                Arc::clone(&borrow.try_connected_point(cur_dir)?)
            };

            // Ic found
            if (ic_knot_measurement - ic_knot_accumulation).so_small() {
                let connection_res = TmeshControlPoint::connect(
                    Arc::clone(&p),
                    Arc::clone(&cur_point),
                    cur_dir.clockwise(),
                    ic_knot_interval,
                );

                // If an existing connection is found, it is possible that the next point over
                // will be a zero knot interval, in which case the connection should go to that point.
                // Any other error should be sent up and if the connection is successful the same thing should happen.
                match connection_res {
                    Ok(()) => return Ok(true),
                    Err(Error::TmeshExistingConnection) => {}
                    Err(e) => return Err(e),
                };

            // Ic not possible, knot accumulation surpassed measurment or reached face corner.
            // Shouldn't need corner detection due to rule 1 in [Sederberg et al. 2003].
            // (needs testing)
            } else if ic_knot_accumulation > ic_knot_measurement
                || cur_point.read().con_type(cur_dir.anti_clockwise()) == TmeshConnectionType::Point
            {
                return Ok(false);
            }
        }
    }

    /// Casts a ray from `p` in the direction `dir` for `num` intersections, returning a vector containing the knot
    /// intervals of each intersection. When an edge condition is encountered before `num` intersections have been
    /// crossed, the returned vector contains the edge knot interval once, after which it is padded with `0.0`.
    /// All vectors returned from this function will have a length `num`.
    ///
    /// # Returns
    /// - `TmeshConnectionNotFound` if a T-mesh is found on the edge of a face, making it non-rectangular (malformed mesh).
    ///
    /// - `TmeshControlPointNotFound` if an edge condition is found inside the mesh or
    ///   if edge condition points are not connected to each other (malformed mesh).
    ///
    /// - `Ok(vec<f64>)` if the ray was successfully cast, returns the knot intervals traversed.
    ///  
    /// # Borrows
    /// Immutably borrows `p` and any points connected to `p` in the direction `dir`, including points which go around any
    /// faces created by T-juctions in the direction `dir`, for `num` perpendicular intersections.
    pub fn cast_ray(
        p: Arc<RwLock<TmeshControlPoint<P>>>,
        dir: TmeshDirection,
        num: usize,
    ) -> Result<Vec<f64>> {
        let mut knot_intervals = Vec::with_capacity(num);
        let mut cur_point = Arc::clone(&p);

        // Some flags for special cases.
        //
        // If an edge condition is found, only the first "intersection" at the edge contion is recorded,
        // and all further deltas are 0, though according to [Sederberg et al. 2003] they do not matter.
        let mut edge_condition_found = false;

        // 'intersection_loop:
        while knot_intervals.len() < num {
            let con_type = cur_point.read().con_type(dir);
            let i = knot_intervals.len();
            knot_intervals.push(0.0);

            match con_type {
                // If dir is a T-junction, navigate around the face to the other side,
                // counting the knot intervals in the direction dir
                TmeshConnectionType::Tjunction => {
                    // Stores the distance traversed away from the ray
                    let mut ray_distance: f64;
                    (cur_point, ray_distance) = {
                        let borrow = cur_point.read();

                        // The possibility that TmeshControlPointNotFound is returned from navigate_until_con would normaly be no
                        // cuase for error, since the other direction may be tried. However, because cur_point is a T junction in
                        // the direction dir, it must be a point connection in dir.anti_clockwise(), otherwise the mesh is malformed.
                        borrow.navigate_until_con(dir.anti_clockwise(), dir)?
                    };

                    // Travrese with counting until a connection in the clockwise connection is found.
                    // Because all faces must be rectangular, this is guaranteed to be the first "ray intersection".
                    let traversal_result =
                        cur_point.read().navigate_until_con(dir, dir.clockwise())?;
                    cur_point = traversal_result.0;
                    // Set the latest pushed value to the intersection length
                    knot_intervals[i] += traversal_result.1;

                    // If a T-junction is encountered, it is (Figure 9 cases aside) guaranteed that on the other side of the face there
                    // is no point which perfectly aligns with the initial point. In this case, a special algorithm must be used to
                    // traverse across the mesh until such a point is found or the requisite number of intersections are reached.
                    // Example below (All distances are in parametric space and represented by physical space between "+", which are points):
                    // <+>---\+/----------------------------------+
                    //  |     |                                   |
                    //  |    [+]-----<+>--+---+-----+--<+>---<+>--+
                    //  |     |       |   |   |     |   |     |   |
                    //  |     |       |   |   |    <+>-<+>    |   |
                    //  |     |       |   |   |     |   |     |   |
                    //  |     |      [+]-(+)-<+>    |  /+\    |   |
                    //  |     |       |       |     |   |    <+>-<+>
                    //  |     |       |      <+>---<+>  |     |   |
                    // {+}~~~~|~~~~~~/+\~~~~~~|~~~~~|~~~|~~~~~|~~|+|
                    //  |     +-------+       |     |   |     |   |
                    //  |     |       |       +----/+\-/+\----+---+
                    //  |     |       |       |                   |
                    //  +-----+-------+-------+-------------------+
                    //  0     1       2       3     4   5     6   7     <-- Intersection numbers, used in comments
                    //
                    // {+} is point from which the ray is "cast"
                    // <+> are points that need to be visited by the algorithm
                    // [+] are the points where if normal ray casting is resumed,
                    //      an incorrect knot vector will be produced.
                    // (+) is a point whose knot interval will be accumulated but not recorded for
                    // |+| is the point at which "normal" ray casting continues (may or may not exist, and
                    //      must not have a T-junction to the right).
                    // /+\ are points which, while closer to the ray in a paramtric sense,
                    //      are not directly accessed for the reasons described in the next paragraph
                    // \+/ is the locatioin of cur_point
                    //  ~  is the "ray"
                    //
                    // In any case, the path taken shall not cross the ray. It can be guaranteed that any edge
                    // the ray pierces will be accessable by this algorithm due to the rectangular nature of the T-mesh.
                    // Lets say that there exists a vertical edge which the ray pierces. That edge must be connected on
                    // either edge to horizontal edges. At the corners, there will be control points. Thus, two of
                    // the control points must be above the ray. Furthermore, to preserve the rectangular nature of
                    // each face, those control points must be connected to two other edges, meaning that at least
                    // one edge from that control point will be pointing up or left, connecting to another edge.
                    // This means that as long as the algorithm used to traverse the mesh stays as close to the ray as possible,
                    // without crossing it, (that is, always stays on a face which is intersected by the ray), there is no
                    // danger of missing an intersection and producing an incorrect knot vector.
                    //
                    // The above code is not included in the loop below because of certain guarantees that can be made about the
                    // geometry of the mesh which cannot be made for the rest of the mesh.
                    'face_traversal: loop {
                        // It is possible that we are traversing along the edge of the mesh, in this case, the below navigate_until_con is
                        // going to navigate until the corner of the mesh, and return an error that it encountered an unexpected
                        // edge condition. This is not actually an error, so it needs to be checked before traversal. In the event that this occurs,
                        // normal ray casting is resumed, since all edge conditions in a mesh have the same weight. Do not push another knot interval,
                        // because the edge arm of the parent match statement will take care of it
                        if cur_point.read().con_type(dir) == TmeshConnectionType::Edge {
                            break 'face_traversal;
                        }

                        if knot_intervals.len() == num {
                            break 'face_traversal;
                        }

                        // Traverse down to the lowest point on this edge which is not a T-junction and has not yet crossed the ray.
                        'ray_approaching: loop {
                            let traversal_result =
                                cur_point.read().navigate_until_con(dir.clockwise(), dir)?;

                            // Subtract distance as we approach the ray (temp var because the result might be
                            // over the ray, in which case we discard it).
                            let new_ray_distance = ray_distance - traversal_result.1;

                            // Found a point where normal ray traversal will continue
                            if new_ray_distance.so_small() {
                                break 'face_traversal;

                            // The detected point crosses the ray, so cur_point is the closest point to the ray with a
                            // connection in the dir direction.
                            } else if new_ray_distance < 0.0 {
                                break 'ray_approaching;
                            }

                            // Move cur_point
                            cur_point = traversal_result.0;
                            // Synchronize distance
                            ray_distance = new_ray_distance;
                        }

                        // It is possble that the above loop exited without modifying cur_point, as is the case for the face marked by
                        // the fourth and fifth intersections above. In this case, cur_point must be navigated up to the corner of the face.
                        if cur_point.read().con_type(dir) == TmeshConnectionType::Tjunction {
                            let traversal_result = cur_point
                                .read()
                                .navigate_until_con(dir.anti_clockwise(), dir)?;

                            // Move cur_point.
                            cur_point = traversal_result.0;
                            // Add distance, since we are traversing away from the ray.
                            ray_distance += traversal_result.1;
                        }

                        // Traverse accross the "top" of the face, to the other corner
                        let traversal_result =
                            cur_point.read().navigate_until_con(dir, dir.clockwise())?;

                        // Record the traversal distance as a knot interval (guaranteed to be correct because all faces are rectangular)
                        knot_intervals.push(traversal_result.1);
                        // Move cur_point
                        cur_point = traversal_result.0;
                    }
                }

                TmeshConnectionType::Point => {
                    // Store knot interval
                    knot_intervals[i] += cur_point.read().connection_knot(dir).expect(
                        "All point connections and edge conditions must have a knot interval",
                    );

                    // Traverse to the next point
                    cur_point = {
                        let borrow = cur_point.read();
                        Arc::clone(&borrow.connected_point(dir))
                    };
                }

                TmeshConnectionType::Edge => {
                    // Edge contition already found, and pushing a zero happens before the match statement, so just continue.
                    if edge_condition_found {
                        continue;
                    }

                    // Store knot interval
                    knot_intervals[i] += cur_point.read().connection_knot(dir).expect(
                        "All point connections and edge conditions must have a knot interval",
                    );

                    // Flag to store zeros for remaining deltas
                    edge_condition_found = true;
                }
            };
        }
        Ok(knot_intervals)
    }
}

impl<P> Tmesh<P>
where P: PartialEq
{
    /// Finds the first point that was added to a T-mesh with a specific cartesian coordinate
    ///
    /// # Returns
    /// - `TmeshControlPointNotFound` if `p` is not found.
    ///
    /// - `Ok(Arc<RwLock<TmeshControlPoint<P>>>)` if the corresponding control point is found.
    ///
    /// # Borrows
    /// Immutably borrows every control point in the `self.control_points`.
    pub fn find(&self, p: P) -> Result<Arc<RwLock<TmeshControlPoint<P>>>> {
        Ok(Arc::clone(
            self.control_points()
                .iter()
                .find(|x| *x.read().point() == p)
                .ok_or(Error::TmeshControlPointNotFound)?,
        ))
    }

    /// Finds a control point with cartesian coordinates `point` and changes them to `new`.
    ///
    /// # Returns
    /// - `TmeshControlPointNotFound` if `point` is not found.
    ///
    /// - `Ok(Arc<RwLock<TmeshControlPoint<P>>>)` if the corresponding control point is found.
    ///
    /// # Borrows
    /// Immutably borrows every point in `self.control_points` and mutably borrows the
    /// control point corresponding to `point` if it is found.
    pub fn map_point(&mut self, point: P, new: P) -> Result<Arc<RwLock<TmeshControlPoint<P>>>> {
        let point = self.find(point)?;
        point.write().set_point(new);
        Ok(point)
    }
}
