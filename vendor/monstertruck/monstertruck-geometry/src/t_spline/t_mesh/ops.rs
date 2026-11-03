use super::basis::basis_function;
use super::*;
use std::fmt;

impl<P> Tmesh<P>
where P: ControlPoint<f64>
{
    /// Attempts to insert a new control point between two existing control points using the technique from \[Sederberg et al. 2003\]
    /// called local knot insertion (LKI), returning the added control point if successful. In order to do so, the knot vectors perpendicular
    /// to the connection for two control points in both directions (including the control points which define the edge) must be equal.
    /// See the figure below for an example.
    ///
    /// ```text
    ///     t1   t2        t3   t4
    ///     +-----+----(+)----+-----+
    ///     |     |           |     |
    ///     +-(+)-+-----------+-----+
    ///     |     |           |     |
    ///  --<+>---{+}---[+]---<+>---<+>--
    ///     |     |           |     |
    ///     +-----+-(+)-------+-----+
    ///     |     |           |     |
    ///     +-----+-----------+-(+)-+
    /// ```
    ///
    /// - `{+}` is `p`, which must exist
    /// - `<+>` are the other points which must exist. Any other points (other than `p`) may or may not exist,
    ///   and LKI will succeed so long as the perpendicular knot vectors are equal for all points `<+>` and `{+}`.
    /// - `[+]` is the point to be inserted.
    /// - `t1 - t5` are the knot vectors perpendicular to the axis of insertion
    /// - `(+)` are points which will not affect or be affected by LKI
    ///
    /// In the above example, the vertical knot vectors t1, t2, t3, and t4 must be equal
    /// (tollerance is used, so exact floating point equality is not nescessary).
    ///
    /// Other points may exist on any of the horizontal connections, so long as they are not on the primary axis
    /// (that would change which points `<+>` or `{+}` would be). Some examples are shown in the diagram as `(+)`.
    /// There can be edges between them, and even induce a connection with the newly inserted point,
    /// which will be automatically added.
    ///
    /// # Returns
    /// - `TmeshControlPointNotFound` if an edge condition is encountered instead of a control point
    ///   along the axis of insertion (Rule 3 \[Sederberg et al. 2003\]).
    ///
    /// - `TmeshConnectionNotFound` if a T-junction is encountered instead of a control point
    ///   along the axis of insertion (Rule 3 \[Sederberg et al. 2003\]).
    ///
    /// - `TmeshInvalidKnotRatio` if `knot_ratio` is not in [0.0, 1.0].
    ///
    /// - `TmeshMalformedMesh` if a knot vector was unable to be constructed for any point.
    ///
    /// - `TmeshKnotVectorsNotEqual` if the knot vectors perpendicular to `dir` are not all equal (Rule 3 \[Sederberg et al. 2003\]).
    ///
    /// - `TmeshConnectionInvalidKnotInterval` if the connection between `p` and the point in the direction `dir` does
    ///   not have the same knot interval in both directions.
    ///
    /// - `Ok(Arc<RwLock<TmeshControlPoint<P>>>)` if the control point was successfully added, where the
    ///   returned control point is the newly added control point
    ///
    /// # Borrows
    /// Immutably borrows two points in the direction `dir` of `p` and one in the direction `dir.flip()`, as well as two points in
    /// either direction perpendicular to `dir` for those points.  
    ///
    /// Mutably borrows `p` and the point connecteed to `p` in the direction `dir`, as well as the newly created control point,
    /// which lies between the two.
    ///
    /// # Notes on Rule 3
    /// Though \[Sederberg et al. 2003\] is not explicitly clear about edge condition (T-junctions imply rule 3 is broken), testing has
    /// revealed that local knot insertion cannot be done on edges connected to a point with one or more edge conditionds.
    pub fn try_local_knot_insertion(
        &mut self,
        p: Arc<RwLock<TmeshControlPoint<P>>>,
        dir: TmeshDirection,
        knot_ratio: f64,
    ) -> Result<Arc<RwLock<TmeshControlPoint<P>>>> {
        match p.read().con_type(dir) {
            TmeshConnectionType::Edge => return Err(Error::TmeshControlPointNotFound),
            TmeshConnectionType::Tjunction => return Err(Error::TmeshConnectionNotFound),
            _ => {}
        };

        if !(0.0..=1.0).contains(&knot_ratio) {
            return Err(Error::TmeshInvalidKnotRatio);
        }

        // Rule 3 of T-splines, [Sederberg et al. 2003], states that all (The paper does not specify existing or otherwise,
        // I am assuming that they may or may not exist, however, the connection from the inner two points must not be
        // a T-junction) perpendicular and in-line knot vectors of length 5 centered on the axis
        // of insertion and a distance of at most two knots from the point to be inserted must be equal. See Figure 10 in
        // [Sederberg et al. 2003] for details.
        let mut center_points: Vec<Arc<RwLock<TmeshControlPoint<P>>>> = Vec::with_capacity(4);

        // An example insertion for reference
        //
        //   --<+>--{+}--[+]---+--<+>--
        //      0    1    ~    2   3   <- center_points and knot_vectors indices
        // {+} is p
        // [+] is the new control point to be inserted
        // <+> may or may not exist (can only insert if they are replaced with edge conditions)
        center_points.push({
            match p.read().con_type(dir.flip()) {
                // Retrieve connected point
                TmeshConnectionType::Point => Arc::clone(&p.read().connected_point(dir.flip())),
                TmeshConnectionType::Edge => return Err(Error::TmeshControlPointNotFound),
                TmeshConnectionType::Tjunction => {
                    return Err(Error::TmeshConnectionNotFound);
                }
            }
        });
        center_points.push(Arc::clone(&p));
        center_points.push({
            let borrow = p.read();
            // Checked in the beginning of the function with match
            Arc::clone(&borrow.connected_point(dir))
        });
        center_points.push({
            let borrow = center_points[2].read();

            match borrow.con_type(dir.flip()) {
                // Retrieve connected point
                TmeshConnectionType::Point => Arc::clone(&borrow.connected_point(dir)),
                TmeshConnectionType::Edge => return Err(Error::TmeshControlPointNotFound),
                TmeshConnectionType::Tjunction => {
                    return Err(Error::TmeshConnectionNotFound);
                }
            }
        });

        // Store the first knot vector to compare it to the rest. If any do not match, return an error
        let knot_vec_compare: KnotVector = {
            let point_knots = Tmesh::point_knot_vectors(Arc::clone(&center_points[1]))?;

            // Depending on the direction of insertion, the S or T knot vectors are needed.
            if dir.horizontal() {
                point_knots.1
            } else {
                point_knots.0
            }
        };
        // Compare knot vectors
        for point in center_points[1..].iter() {
            // Get knot vectors in both directions for the point
            let point_knots = Tmesh::point_knot_vectors(Arc::clone(point))
                .map_err(|_| Error::TmeshMalformedMesh)?;

            // Depending on the direction of insertion, the S or T knot vectors are needed.
            let cur_kv = if dir.horizontal() {
                point_knots.1
            } else {
                point_knots.0
            };

            // Compare knot vectors using so_small because knot vector construction uses
            // knot intervals which are prone to small errors.
            if !cur_kv
                .iter()
                .zip(knot_vec_compare.iter())
                .all(|t| (t.0 - t.1).so_small())
            {
                return Err(Error::TmeshKnotVectorsNotEqual);
            }
        }

        // Get d1 - d6. See Figure 10 in [Sederberg et al. 2003].
        let mut d: Vec<f64> = Vec::with_capacity(6);
        // d1 and d2
        for point in center_points[0..2].iter() {
            d.push(
                Tmesh::cast_ray(Arc::clone(point), dir.flip(), 1)
                    .map_err(|_| Error::TmeshMalformedMesh)?[0],
            );
        }
        // d3
        d.push(
            center_points[1]
                .read()
                .connection_knot(dir)
                .ok_or(Error::TmeshConnectionNotFound)?
                * knot_ratio,
        );
        // d4
        d.push(d.last().expect("Vector should not be empty") * ((1.0 / knot_ratio) - 1.0));
        // d5 and d6
        for point in center_points[2..4].iter() {
            d.push(
                Tmesh::cast_ray(Arc::clone(point), dir, 1)
                    .map_err(|_| Error::TmeshMalformedMesh)?[0],
            );
        }

        let cartesian_points: Vec<P> = center_points.iter().map(|p| *p.read().point()).collect();

        // Equations 5, 6, and 7 from [Sederberg et al. 2003]. Remember that P3 is not a point in either
        // cartesian_points or center_points, and arrays in rust are 0 indexed,
        let p2_prime = ((cartesian_points[0] * d[3])
            + (cartesian_points[1].to_vec() * (d[0] + d[1] + d[2])))
            / (d[0] + d[1] + d[2] + d[3]);

        let p4_prime = ((cartesian_points[3] * d[2])
            + (cartesian_points[2].to_vec() * (d[3] + d[4] + d[5])))
            / (d[2] + d[3] + d[4] + d[5]);

        let p3_prime = ((cartesian_points[1] * (d[3] + d[4]))
            + (cartesian_points[2].to_vec() * (d[1] + d[2])))
            / (d[1] + d[2] + d[3] + d[4]);

        center_points[1].write().set_point(p2_prime);

        center_points[2].write().set_point(p4_prime);

        self.add_control_point(p3_prime, Arc::clone(&p), dir, knot_ratio)
    }

    /// Absolute knot coordinate interface for local knot insertion (LKI). Tries to insert a control point
    /// at the specified absolute knot coordinates `knot_coords` without changing the shape of the resulting surface.
    /// For details on LKI, see [`Tmesh::try_local_knot_insertion()`]. In order for the function to succeed, an edge must
    /// exist which passes through the knot coordinates `knot_coords`, that is, either two vertical points or horizontal
    /// points straddle the parametric coordinates where the new point is to be inserted.
    ///
    /// # Returns
    /// - `TmeshOutOfBoundsInsertion` if either component of `knot_coords` is not in the range `(0.0, 1.0)`.
    ///
    /// - `TmeshExistingControlPoint` if a control point already exists at the parametric coordinates `knot_coords`.
    ///
    /// - `TmeshMalformedMesh` if intersecting edges are found.
    ///
    /// - `TmeshConnectionNotFound` if no edges are found intersecting the knot coordinates `knot_coords`.
    ///
    /// # Borrows
    /// Immutably borrows every control point in `self`, immutably borrows two points in the direction `dir` of `p`
    /// and one in the direction `dir.flip()`, as well as two points in either direction perpendicular to `dir` for those points.  
    ///
    /// Mutably borrows the two control points which straddle the knot coordinates `knot_coords`, as well as the newly created control point,
    /// which lies at those knot coordinates.
    pub fn try_absolute_local_knot_insertion(
        &mut self,
        knot_coords: (f64, f64),
    ) -> Result<Arc<RwLock<TmeshControlPoint<P>>>> {
        // Make sure desred knot coordinates are within msh bounds
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
                return self.try_local_knot_insertion(
                    Arc::clone(&s_axis_straddle_points[0]),
                    TmeshDirection::Up,
                    (knot_coords.1 - point_t_coord) / con_knot,
                );
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
                self.try_local_knot_insertion(
                    Arc::clone(&t_axis_straddle_points[0]),
                    TmeshDirection::Right,
                    (knot_coords.0 - point_s_coord) / con_knot,
                )
            }
            _ => {
                // Multiple S-edges are found where the point intersects (Should never happen)
                Err(Error::TmeshMalformedMesh)
            }
        }
    }

    /// Convenience wrapper for local knot insertion that automatically inserts intermediate edges
    /// when `try_absolute_local_knot_insertion` fails due to no straddling edge existing at `(s, t)`.
    ///
    /// The method first attempts direct insertion. If no edge straddles the target coordinates, it
    /// scans the mesh for the nearest horizontal or vertical edge that could be extended through
    /// the target point, inserts intermediate control points along that edge using LKI, and retries.
    ///
    /// This is shape-preserving: the surface is unchanged after refinement.
    ///
    /// # Returns
    /// - `TmeshOutOfBoundsInsertion` if coordinates are outside `[0.0, 1.0]`.
    /// - `TmeshExistingControlPoint` if a point already exists at the target.
    /// - `TmeshConnectionNotFound` if no suitable edges can be found even after intermediate insertions.
    /// - `Ok(Arc<RwLock<TmeshControlPoint<P>>>)` on success.
    ///
    /// # Borrows
    /// See [`Tmesh::try_absolute_local_knot_insertion`].
    pub fn refine_at(&mut self, s: f64, t: f64) -> Result<Arc<RwLock<TmeshControlPoint<P>>>> {
        // Try direct insertion first.
        match self.try_absolute_local_knot_insertion((s, t)) {
            Ok(cp) => return Ok(cp),
            Err(Error::TmeshConnectionNotFound) => {}
            Err(e) => return Err(e),
        }

        // No straddling edge found. Create one by inserting a full column or row of
        // intermediate points using LKI. Inferred connections (Rule 2) require matching
        // points on opposite face edges, so we must insert at ALL t-levels (or s-levels)
        // to build a connected column (or row). Insertions are done bottom-to-top (or
        // left-to-right) so each successive point finds its predecessor via Rule 2.

        // Strategy A: Insert a vertical column at s by finding all horizontal edges
        // that straddle s and inserting LKI points at (s, t_level) for each.
        let mut h_t_levels: Vec<f64> = Vec::new();
        for cp in self.control_points.iter() {
            let r = cp.read();
            if let Some(con) = r.get(TmeshDirection::Right) {
                let cp_s = r.knot_coordinates().0;
                let cp_t = r.knot_coordinates().1;
                let ki = con.1;
                if cp_s < s && cp_s + ki > s {
                    h_t_levels.push(cp_t);
                }
            }
        }
        // Sort bottom-to-top so inferred connections chain upwards.
        // SAFETY: knot coordinates are finite `f64` values, so `partial_cmp` always returns `Some`.
        h_t_levels.sort_by(|a, b| a.partial_cmp(b).unwrap());
        h_t_levels.dedup_by(|a, b| (*a - *b).so_small());

        if h_t_levels.len() >= 2 {
            for &t_level in &h_t_levels {
                self.try_absolute_local_knot_insertion((s, t_level))?;
            }
            // Retry -- a vertical column now exists at s with edges straddling t.
            return self.try_absolute_local_knot_insertion((s, t));
        }

        // Strategy B: Insert a horizontal row at t.
        let mut v_s_levels: Vec<f64> = Vec::new();
        for cp in self.control_points.iter() {
            let r = cp.read();
            if let Some(con) = r.get(TmeshDirection::Up) {
                let cp_s = r.knot_coordinates().0;
                let cp_t = r.knot_coordinates().1;
                let ki = con.1;
                if cp_t < t && cp_t + ki > t {
                    v_s_levels.push(cp_s);
                }
            }
        }
        // SAFETY: knot coordinates are finite `f64` values, so `partial_cmp` always returns `Some`.
        v_s_levels.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v_s_levels.dedup_by(|a, b| (*a - *b).so_small());

        if v_s_levels.len() >= 2 {
            for &s_level in &v_s_levels {
                self.try_absolute_local_knot_insertion((s_level, t))?;
            }
            return self.try_absolute_local_knot_insertion((s, t));
        }

        Err(Error::TmeshConnectionNotFound)
    }

    /// Returns the cartesian point corresponding to the parametric coordinates for `self`. Usually the
    /// parametric coordinates are constrained from 0 to 1 for both `s` and `t` as this is the domain of
    /// the T-mesh in parametric space. However, parameters are not checked or forcefully constrained,
    /// as there is a domain of continuity outside the usual parameter range. This domain, however, is not
    /// guaranteed, and should be accessed at your own risk.
    ///
    /// # Returns
    /// - `TmeshConnectionNotFound` if `self` contains a non-rectangular grid, in which case generating knot vectors will fail.
    ///
    /// - `TmeshControlPointNotFound` if `self` contains an edge condition inside of its mesh.
    ///
    /// - `Ok(P)` if the calculation succeeded. A `P` will be returned which is the T-mesh transformation
    ///   of `(s, t)` into cartesian space.
    ///
    /// # Borrows
    /// Immutably borrows every control point in `self`.
    pub fn subs(&self, s: f64, t: f64) -> Result<P> {
        // Generate knot vectors if stale.
        if self.knot_vectors.read().is_none() {
            self.generate_knot_vectors()?;
        }

        let borrow = self.knot_vectors.read();
        let all_kvs = borrow
            .as_ref()
            .expect("Knot vectors should have successfully generated or an error returned");

        let num = self.control_points.len();
        let basis_evaluations: Vec<f64> = all_kvs
            .iter()
            .take(num)
            .map(|kvs| basis_function(s, kvs.0.as_slice()) * basis_function(t, kvs.1.as_slice()))
            .collect();

        let numerator = basis_evaluations
            .iter()
            .zip(self.control_points().iter().map(|c| *c.read().point()))
            .fold(P::origin(), |sum, (b, p)| sum + p.to_vec() * *b);

        let denominator: f64 = basis_evaluations.iter().sum();
        Ok(numerator / denominator)
    }
}

impl<P> fmt::Display for Tmesh<P> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // If only Hash Maps could use f64....
        #[allow(clippy::type_complexity)]
        let mut s_levels: Vec<(f64, Vec<Arc<RwLock<TmeshControlPoint<P>>>>)> = Vec::new();
        #[allow(clippy::type_complexity)]
        let mut t_levels: Vec<(f64, Vec<Arc<RwLock<TmeshControlPoint<P>>>>)> = Vec::new();

        let sort_f64 = |a: &f64, b: &f64| -> std::cmp::Ordering {
            if (a - b).so_small() {
                return std::cmp::Ordering::Equal;
            } else if a > b {
                return std::cmp::Ordering::Greater;
            }
            std::cmp::Ordering::Less
        };

        for point in self.control_points.iter() {
            let coords = point.read().knot_coordinates();

            if let Some(s_level) = s_levels
                .iter_mut()
                .find(|c| sort_f64(&c.0, &coords.0) == std::cmp::Ordering::Equal)
            {
                let point_vec: &mut Vec<Arc<RwLock<TmeshControlPoint<P>>>> = s_level.1.as_mut();
                point_vec.push(Arc::clone(point));
            } else {
                s_levels.push((coords.0, Vec::new()));
                s_levels
                    .last_mut()
                    .expect("Pushed element on previous line.")
                    .1
                    .push(Arc::clone(point));
            }

            if let Some(t_level) = t_levels
                .iter_mut()
                .find(|c| sort_f64(&c.0, &coords.1) == std::cmp::Ordering::Equal)
            {
                let point_vec: &mut Vec<Arc<RwLock<TmeshControlPoint<P>>>> = t_level.1.as_mut();
                point_vec.push(Arc::clone(point));
            } else {
                t_levels.push((coords.1, Vec::new()));
                t_levels
                    .last_mut()
                    .expect("Pushed element on previous line.")
                    .1
                    .push(Arc::clone(point));
            }
        }

        s_levels.sort_unstable_by(|a, b| sort_f64(&a.0, &b.0));
        t_levels.sort_unstable_by(|a, b| sort_f64(&a.0, &b.0));

        t_levels = t_levels.into_iter().rev().collect();

        let mut vertical_cons: Vec<bool> = vec![false; s_levels.len()];
        for (i, (s_level, _)) in s_levels.iter().enumerate() {
            if let Some(point) = t_levels[0]
                .1
                .iter()
                .find(|p| p.read().knot_coordinates().0 == *s_level)
            {
                vertical_cons[i] =
                    point.read().con_type(TmeshDirection::Up) != TmeshConnectionType::Tjunction;
            }
        }
        write!(f, "       ")?;
        let mut line = String::new();
        for con in vertical_cons.iter() {
            if *con {
                line.push_str("|   ");
            } else {
                line.push_str("    ");
            }
        }
        writeln!(f, "{}", line)?;

        // let line_len = 2 * s_levels.len();
        for t_level in t_levels {
            let mut line = String::new();
            let mut has_left_edge = false;
            let mut has_right_edge = false;

            for (i, (s_level, _)) in s_levels.iter().enumerate() {
                if let Some(point) = t_level
                    .1
                    .iter()
                    .find(|p| p.read().knot_coordinates().0 == *s_level)
                {
                    if point.read().con_type(TmeshDirection::Left) == TmeshConnectionType::Edge {
                        line.push_str("--");
                        has_left_edge = true;
                    }

                    line.push('+');
                    vertical_cons[i] = point.read().con_type(TmeshDirection::Down)
                        != TmeshConnectionType::Tjunction;
                    line.push_str(match point.read().con_type(TmeshDirection::Right) {
                        TmeshConnectionType::Edge => "--",
                        TmeshConnectionType::Point => {
                            has_right_edge = true;
                            "---"
                        }
                        TmeshConnectionType::Tjunction => {
                            has_right_edge = false;
                            "   "
                        }
                    });
                } else if vertical_cons[i] {
                    line.push_str("|   ");
                } else if has_right_edge {
                    line.push_str("----");
                } else {
                    line.push_str("    ");
                }
            }

            write!(f, "{:.2} ", t_level.0)?;
            if !has_left_edge {
                write!(f, "  ")?;
            }
            writeln!(f, "{}", line)?;

            write!(f, "       ")?;
            let mut line = String::new();
            for con in vertical_cons.iter() {
                if *con {
                    line.push_str("|   ");
                } else {
                    line.push_str("    ");
                }
            }
            writeln!(f, "{}", line)?;
        }

        let mut s_demarcations = (
            format!("{:.2}", s_levels[0].0),
            format!("{:.2}", s_levels[1].0),
        );
        for (i, s_level) in s_levels[2..].iter().enumerate() {
            if i % 2 == 0 {
                s_demarcations
                    .0
                    .push(if vertical_cons[i + 1] { '|' } else { ' ' });
                s_demarcations
                    .0
                    .push_str(format!("   {:.2}", s_level.0).as_str());
            } else {
                s_demarcations
                    .1
                    .push_str(format!("    {:.2}", s_level.0).as_str());
            }
        }

        if *vertical_cons
            .last()
            .expect("All T-meshes have at least 2 S-levels")
            && s_levels.len().is_multiple_of(2)
        {
            s_demarcations.0.push('|');
        }

        write!(f, "       ")?;
        writeln!(f, "{}", s_demarcations.0)?;
        write!(f, "           ")?;
        writeln!(f, "{}", s_demarcations.1)?;
        Ok(())
    }
}

impl<P> Tmesh<P>
where P: Clone
{
    /// Subdivides a mesh by inserting a new control point parametrically halfway between every pair of connected control points
    /// already present in the mesh. This includes any implicit edges created during the subdivision of the mesh. Thus, a 2x2
    /// mesh created with the `new` function will become a 3x3 mesh with a point in the center of the mesh. The cartesian coordinates
    /// of the new control points is determined with a caller-specified closure, `f`, which will be given the two control points
    /// which will be on either side of the new control point. The first point parameter passed to `f` will always be either the
    /// left or bottom control point in a pair, depending on the edge being subdivided.
    ///
    /// # Returns
    /// - `TmeshConnectionInvalidKnotInterval` if a connection is found which has mismatched knot intervals
    ///   depending on which point in the connection is referenced.
    ///
    /// - `Ok()` if the mesh was successfully subdivided.
    ///
    /// # Borrows
    /// Mutably borrows every control point in `self.control_points`.
    pub fn subdivide<F>(&mut self, f: F) -> Result<()>
    where F: Fn(P, P) -> P {
        // Get all (pairs of) control points with horizontal point to point connections
        let righties: Vec<_> = self
            .control_points()
            .iter()
            .filter(|p| p.read().con_type(TmeshDirection::Right) == TmeshConnectionType::Point)
            .map(Arc::clone)
            .collect();

        // Split all the connections in two
        for cont_p in righties {
            // Get the new control point using the caller supplied closure
            let p = f(
                cont_p.read().point().clone(),
                cont_p
                    .read()
                    .connected_point(TmeshDirection::Right)
                    .read()
                    .point()
                    .clone(),
            );

            self.add_control_point(p, Arc::clone(&cont_p), TmeshDirection::Right, 0.5)?;
        }

        // The above for loop will create new connections in the DOWN direction through implicit connections.
        // Thus, the filtering of the downies must happen after addiing the righties.
        let uppies: Vec<_> = self
            .control_points()
            .iter()
            .filter(|p| p.read().con_type(TmeshDirection::Up) == TmeshConnectionType::Point)
            .map(Arc::clone)
            .collect();

        for cont_p in uppies {
            let p = f(
                cont_p.read().point().clone(),
                cont_p
                    .read()
                    .connected_point(TmeshDirection::Up)
                    .read()
                    .point()
                    .clone(),
            );

            self.add_control_point(p, Arc::clone(&cont_p), TmeshDirection::Up, 0.5)?;
        }

        Ok(())
    }
}

impl<P> Clone for Tmesh<P>
where P: Clone
{
    fn clone(&self) -> Tmesh<P> {
        // Vector containing new point objects which have the same positions as the points in the original mesh
        let mut points_copy = Vec::new();
        // Vector containing the connections for each point with the corresponding index in points_copy.
        // Each sub-vector will be 4 elements long, and each element of the sub-vector will be None if the
        // connection is a T-junction, Some((None, f64)) for an Edge condition, and Some((Some(index), f64))
        // for a Point connection, where index is the index of the connected point in self.control_points,
        // and thus points_copy by extension.
        #[allow(clippy::type_complexity)]
        let mut point_connections: Vec<Vec<Option<(Option<usize>, f64)>>> = Vec::new();

        // Copy all the points into points_copy and all connections into point_connections
        for point in self.control_points.iter() {
            // Clone the cartesian point
            let cart_point = {
                let borrow = point.read();
                borrow.point().clone()
            };
            // Push a new control point corresponding to the control point in self.control_points to points_copy
            // The edge interval is 1.0, however, this can be any value, since establishing connections will
            // overwrite this with the correct value.
            points_copy.push(Arc::new(RwLock::new(TmeshControlPoint::new(
                cart_point, 1.0,
            ))));

            // Push a new set of connections
            point_connections.push(Vec::new());
            // Retrieve the previously pushed set of connections for ease of use.
            let last = point_connections
                .last_mut()
                .expect("Previously pushed item");

            // TmeshDirection::iter() produces the same order of directions every time, so all connection
            // sub-vectors in point_connections will be ordered in the same way, and will be read the same
            // way during connection establishment.
            for dir in TmeshDirection::iter() {
                match point.read().con_type(dir) {
                    // Some((None, f64))
                    TmeshConnectionType::Edge => last.push(Some((
                        None,
                        point
                            .read()
                            .connection_knot(dir)
                            .expect("Edge connection types must have a knot interval."),
                    ))),
                    // Some((Some(Index), f64))
                    TmeshConnectionType::Point => {
                        let connected_point = point.read().connected_point(dir);

                        last.push(Some(
                        (Some(
                            self.control_points
                                .iter()
                                .position(|p| std::ptr::eq(p.as_ref(), connected_point.as_ref())).expect("All connected points must be stored in tmesh control_points vector"),
                        ), point.read().connection_knot(dir).expect("Point connection types must have a knot interval.")),
                    ))
                    }
                    // None
                    TmeshConnectionType::Tjunction => {
                        last.push(None);
                    }
                };
            }
        }

        // Establish connections
        // 'points_loop:
        for (point_index, connections) in point_connections.iter().enumerate() {
            // Zip direction with corresponding connections to index the direction for modification
            'connections_loop: for (connection, dir) in
                connections.iter().zip(TmeshDirection::iter())
            {
                if let Some(con) = connection {
                    // Point connection
                    if let Some(con_index) = con.0 {
                        // Connections has already been established. Connect will also add the connection to points_copy[con_index],
                        // so when points_copy[con_index] is reached by 'points_loop, the connection will already exist, so we skip it.
                        if points_copy[point_index].read().con_type(dir)
                            == TmeshConnectionType::Point
                        {
                            continue 'connections_loop;
                        }

                        // Remove existing edge conditions from both points to be connected.
                        {
                            points_copy[point_index]
                                .write()
                                .remove_connection(dir)
                                .expect("Connections are only modified once.");
                            points_copy[con_index]
                                .write()
                                .remove_connection(dir.flip())
                                .expect("Connections are only modified once.");
                        }

                        // Connect points to each other
                        TmeshControlPoint::connect(
                            Arc::clone(&points_copy[point_index]),
                            Arc::clone(&points_copy[con_index]),
                            dir,
                            con.1,
                        )
                        .expect("Control points have no connections between each other.")
                    // Edge condition
                    } else {
                        points_copy[point_index]
                            .write()
                            .set_edge_con_weight(dir, con.1)
                            .expect(
                                "Unmodified control points have edge conditions in all directions.",
                            );
                    }
                // T-junction
                } else {
                    points_copy[point_index]
                        .write()
                        .remove_connection(dir)
                        .expect(
                            "Unmodified control points have edge conditions in all directions.",
                        );
                }
            }
        }

        // Set absolute knot coordinates
        for (i, p) in self.control_points().iter().enumerate() {
            points_copy[i].write().knot_coordinates = p.read().knot_coordinates();
        }

        Tmesh {
            control_points: points_copy,
            knot_vectors: RwLock::new(None),
        }
    }
}

impl<T> Drop for Tmesh<T> {
    fn drop(&mut self) {
        // Destroy all connections in the mesh so that the only remaining reference to all the points is in
        // self.control_points to prevent leaks
        for p in self.control_points.iter() {
            for dir in TmeshDirection::iter() {
                let _ = p.write().remove_connection(dir);
            }
        }
    }
}

impl<T> Tmesh<T>
where T: Debug + Clone
{
    /// Prints the knot vectors for every point in the mesh.
    ///
    /// # Borrows
    /// Immutably borrows every point in `self.control_points`
    pub fn print_knot_vectors(&self) {
        for point in self.control_points() {
            let cart = {
                let borrow = point.read();
                (*borrow.point()).clone()
            };
            let knot_vectors =
                Tmesh::point_knot_vectors(Arc::clone(point)).expect("Mesh should not be malformed");
            println!("{:?}", cart);
            println!("\tS: {:?}", knot_vectors.0);
            println!("\tT: {:?}", knot_vectors.1);
            println!();
        }
    }
}
impl<P> Tmesh<P>
where P: ControlPoint<f64> + Debug + Clone
{
    /// Creates a T-mesh from a quad mesh by converting to a T-NURCC, applying
    /// CC subdivision, and extracting a parametric surface patch.
    ///
    /// # Arguments
    /// * `positions` - Vertex positions of the quad mesh.
    /// * `quad_faces` - Quad face indices (each face is 4 vertex indices, CCW winding).
    /// * `subdivision_levels` - Number of CC subdivision iterations.
    pub fn from_quad_mesh(
        positions: Vec<P>,
        quad_faces: &[[usize; 4]],
        subdivision_levels: usize,
    ) -> Result<Self> {
        let tnurcc = Tnurcc::from_quad_mesh(positions, quad_faces)?;
        tnurcc.to_tmesh(subdivision_levels)
    }

    /// Converts a cubic `BsplineSurface` into a T-mesh with a regular rectangular grid.
    ///
    /// Any cubic B-spline surface is trivially a T-spline with no T-junctions.
    /// This enables converting existing NURBS/B-spline geometry into T-splines
    /// for further refinement or editing.
    ///
    /// # Errors
    /// Returns `TmeshNonCubicDegree` if the surface is not degree 3 in both directions.
    pub fn from_bspline_surface(surface: &BsplineSurface<P>) -> Result<Self> {
        let (udeg, vdeg) = surface.degrees();
        if udeg != 3 || vdeg != 3 {
            return Err(Error::TmeshNonCubicDegree(udeg, vdeg));
        }

        let u_kv = surface.knot_vector_u();
        let v_kv = surface.knot_vector_v();
        let cps = surface.control_points();
        let nv = cps[0].len();

        // Normalize knot values to [0,1].
        let u_min = u_kv[0];
        let u_range = u_kv[u_kv.len() - 1] - u_min;
        let v_min = v_kv[0];
        let v_range = v_kv[v_kv.len() - 1] - v_min;

        let norm_u = |idx: usize| -> f64 {
            if u_range.so_small() {
                0.5
            } else {
                (u_kv[idx + 2] - u_min) / u_range
            }
        };
        let norm_v = |idx: usize| -> f64 {
            if v_range.so_small() {
                0.5
            } else {
                (v_kv[idx + 2] - v_min) / v_range
            }
        };

        // Create the grid of T-mesh control points.
        let grid: Vec<Vec<Arc<RwLock<TmeshControlPoint<P>>>>> = cps
            .iter()
            .enumerate()
            .map(|(i, row_cps)| {
                row_cps
                    .iter()
                    .enumerate()
                    .map(|(j, cp)| {
                        Arc::new(RwLock::new(TmeshControlPoint {
                            point: *cp,
                            connections: [
                                Some((None, 0.0)),
                                Some((None, 0.0)),
                                Some((None, 0.0)),
                                Some((None, 0.0)),
                            ],
                            knot_coordinates: (norm_u(i), norm_v(j)),
                        }))
                    })
                    .collect()
            })
            .collect();
        let all_points: Vec<Arc<RwLock<TmeshControlPoint<P>>>> = grid
            .iter()
            .flat_map(|row| row.iter().map(Arc::clone))
            .collect();

        // Connect adjacent points horizontally (Right/Left).
        for (i, pair) in grid.windows(2).enumerate() {
            let ki = norm_u(i + 1) - norm_u(i);
            for (left, right) in pair[0].iter().zip(pair[1].iter()) {
                {
                    let mut w = left.write();
                    w.connections[TmeshDirection::Right as usize] =
                        Some((Some(Arc::clone(right)), ki));
                }
                {
                    let mut w = right.write();
                    w.connections[TmeshDirection::Left as usize] =
                        Some((Some(Arc::clone(left)), ki));
                }
            }
        }

        // Connect adjacent points vertically (Up/Down).
        for j in 0..nv - 1 {
            let ki = norm_v(j + 1) - norm_v(j);
            for row in &grid {
                {
                    let mut w = row[j].write();
                    w.connections[TmeshDirection::Up as usize] =
                        Some((Some(Arc::clone(&row[j + 1])), ki));
                }
                {
                    let mut w = row[j + 1].write();
                    w.connections[TmeshDirection::Down as usize] =
                        Some((Some(Arc::clone(&row[j])), ki));
                }
            }
        }

        // Set edge condition weights on boundary points.
        for row in &grid {
            for cell in row {
                let mut w = cell.write();
                for dir in TmeshDirection::iter() {
                    let di = dir as usize;
                    let is_zero_edge = w.connections[di]
                        .as_ref()
                        .is_some_and(|c| c.0.is_none() && c.1 == 0.0);
                    if !is_zero_edge {
                        continue;
                    }
                    // Use the nearest interior connection's knot interval.
                    let weight = [dir.flip(), dir.clockwise(), dir.anti_clockwise()]
                        .iter()
                        .filter_map(|&d| {
                            w.connections[d as usize]
                                .as_ref()
                                .and_then(|c| c.0.is_some().then_some(c.1))
                        })
                        .next()
                        .unwrap_or(0.1);
                    w.connections[di] = Some((None, weight));
                }
            }
        }

        Ok(Tmesh {
            control_points: all_points,
            knot_vectors: RwLock::new(None),
        })
    }
}
