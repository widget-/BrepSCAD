#![allow(clippy::many_single_char_names)]

use super::*;
use rustc_hash::FxHashMap as HashMap;
use truck_base::cgmath64::*;
use truck_geometry::prelude::*;
use truck_meshalgo::prelude::*;
use truck_topology::{Vertex, *};

type PolylineCurve = truck_meshalgo::prelude::PolylineCurve<Point3>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapesOpStatus {
    Unknown,
    And,
    Or,
}

impl ShapesOpStatus {
    fn not(self) -> Self {
        match self {
            Self::Unknown => Self::Unknown,
            Self::And => Self::Or,
            Self::Or => Self::And,
        }
    }
}

#[derive(Clone, Debug)]
pub struct BoundaryWire<P, C> {
    wire: Wire<P, C>,
    status: ShapesOpStatus,
}

impl<P, C> BoundaryWire<P, C> {
    #[inline(always)]
    pub fn new(wire: Wire<P, C>, status: ShapesOpStatus) -> Self { Self { wire, status } }
    #[inline(always)]
    pub fn status(&self) -> ShapesOpStatus { self.status }
    #[inline(always)]
    pub fn invert(&mut self) {
        self.wire.invert();
        self.status = self.status.not();
    }
    #[inline(always)]
    pub fn inverse(&self) -> Self {
        Self {
            wire: self.wire.inverse(),
            status: self.status.not(),
        }
    }
}

impl ShapesOpStatus {
    fn from_is_curve<C, S0, S1>(curve: &IntersectionCurve<C, S0, S1>) -> Option<ShapesOpStatus>
    where
        C: ParametricCurve3D + BoundedCurve,
        S0: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
        S1: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>, {
        let (t0, t1) = curve.range_tuple();
        let t = (t0 + t1) / 2.0;
        let (_, pt0, pt1) = curve.search_triple(t, 100)?;
        let der = curve.leader().der(t);
        let normal0 = curve.surface0().normal(pt0[0], pt0[1]);
        let normal1 = curve.surface1().normal(pt1[0], pt1[1]);
        match normal0.cross(der).dot(normal1) > 0.0 {
            true => Some(ShapesOpStatus::Or),
            false => Some(ShapesOpStatus::And),
        }
    }
}

impl<P, C> std::ops::Deref for BoundaryWire<P, C> {
    type Target = Wire<P, C>;
    #[inline(always)]
    fn deref(&self) -> &Self::Target { &self.wire }
}

impl<P, C> std::ops::DerefMut for BoundaryWire<P, C> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.wire }
}

#[derive(Clone, Debug)]
pub struct Loops<P, C>(Vec<BoundaryWire<P, C>>);
#[derive(Clone, Debug)]
pub struct LoopsStore<P, C>(Vec<Loops<P, C>>);

impl<P, C> std::ops::Deref for Loops<P, C> {
    type Target = Vec<BoundaryWire<P, C>>;
    #[inline(always)]
    fn deref(&self) -> &Self::Target { &self.0 }
}

impl<P, C> std::ops::DerefMut for Loops<P, C> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}

impl<P, C> std::ops::Deref for LoopsStore<P, C> {
    type Target = Vec<Loops<P, C>>;
    #[inline(always)]
    fn deref(&self) -> &Self::Target { &self.0 }
}

impl<P, C> std::ops::DerefMut for LoopsStore<P, C> {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
}

impl<P, C> FromIterator<BoundaryWire<P, C>> for Loops<P, C> {
    #[inline(always)]
    fn from_iter<I: IntoIterator<Item = BoundaryWire<P, C>>>(iter: I) -> Self {
        Self(Vec::from_iter(iter))
    }
}

impl<'a, P, C, S> From<&'a Face<P, C, S>> for Loops<P, C> {
    #[inline(always)]
    fn from(face: &'a Face<P, C, S>) -> Loops<P, C> {
        face.absolute_boundaries()
            .iter()
            .map(|wire| BoundaryWire::new(wire.clone(), ShapesOpStatus::Unknown))
            .collect()
    }
}

impl<'a, P: 'a, C: 'a, S: 'a> FromIterator<&'a Face<P, C, S>> for LoopsStore<P, C> {
    fn from_iter<I: IntoIterator<Item = &'a Face<P, C, S>>>(iter: I) -> Self {
        Self(iter.into_iter().map(Loops::from).collect())
    }
}

impl<'a, P, C> IntoIterator for &'a LoopsStore<P, C> {
    type Item = <&'a Vec<Loops<P, C>> as IntoIterator>::Item;
    type IntoIter = <&'a Vec<Loops<P, C>> as IntoIterator>::IntoIter;
    fn into_iter(self) -> Self::IntoIter { self.0.iter() }
}

#[derive(Clone, Debug, Copy, PartialEq)]
enum ParameterKind {
    Front,
    Back,
    Inner(f64),
}

impl ParameterKind {
    fn try_new(t: f64, (t0, t1): (f64, f64)) -> Option<ParameterKind> {
        if t0.near(&t) {
            Some(ParameterKind::Front)
        } else if t1.near(&t) {
            Some(ParameterKind::Back)
        } else if t0 < t && t < t1 {
            Some(ParameterKind::Inner(t))
        } else {
            None
        }
    }
}

impl<P: Copy, C: Clone> Loops<P, C> {
    fn search_parameter(&self, pt: P) -> Option<(usize, usize, ParameterKind)>
    where C: BoundedCurve<Point = P> + SearchParameter<D1, Point = P> {
        self.iter()
            .enumerate()
            .flat_map(move |(i, wire)| wire.iter().enumerate().map(move |(j, edge)| (i, j, edge)))
            .find_map(|(i, j, edge)| {
                let curve = edge.curve();
                curve.search_parameter(pt, None, 1).and_then(|t| {
                    let kind = ParameterKind::try_new(t, curve.range_tuple())?;
                    Some((i, j, kind))
                })
            })
    }

    fn change_vertex(
        &mut self,
        old_vertex: &Vertex<P>,
        new_vertex: &Vertex<P>,
        emap: &mut HashMap<EdgeID<C>, Edge<P, C>>,
    ) {
        self.iter_mut()
            .flat_map(|wire| wire.iter_mut())
            .for_each(|edge| {
                let mut new_edge = if edge.absolute_front() == old_vertex {
                    emap.entry(edge.id()).or_insert_with(|| {
                        Edge::new(new_vertex, edge.absolute_back(), edge.curve())
                    })
                } else if edge.absolute_back() == old_vertex {
                    emap.entry(edge.id()).or_insert_with(|| {
                        Edge::new(edge.absolute_front(), new_vertex, edge.curve())
                    })
                } else {
                    return;
                }
                .clone();
                if !edge.orientation() {
                    new_edge.invert();
                }
                // Remove the edge from the HashMap when it is no longer there because ID reassignment will occur.
                if edge.count() == 1 {
                    emap.remove(&edge.id());
                }
                *edge = new_edge;
            })
    }

    fn swap_edge_into_wire(&mut self, edge_id: EdgeID<C>, new_wire: &Wire<P, C>) {
        self.iter_mut().for_each(|wire| {
            let mut iter = wire.iter().enumerate();
            if let Some((idx, edge)) = iter.find(|(_, edge)| edge.id() == edge_id) {
                if edge.orientation() {
                    wire.swap_edge_into_wire(idx, new_wire.clone());
                } else {
                    wire.swap_edge_into_wire(idx, new_wire.inverse());
                }
            }
        });
    }

    #[inline(always)]
    fn add_independent_loop(&mut self, r#loop: BoundaryWire<P, C>) {
        self.push(r#loop.inverse());
        self.push(r#loop);
    }

    fn add_edge(
        &mut self,
        edge0: Edge<P, C>,
        status: ShapesOpStatus,
    ) -> [Option<(usize, usize)>; 2] {
        let a = self.iter().enumerate().find_map(|(i, wire)| {
            wire.iter().enumerate().find_map(|(j, edge)| {
                if edge.front() == edge0.back() {
                    Some((i, j))
                } else {
                    None
                }
            })
        });
        let b = self.iter().enumerate().find_map(|(i, wire)| {
            wire.iter().enumerate().find_map(|(j, edge)| {
                if edge.front() == edge0.front() {
                    Some((i, j))
                } else {
                    None
                }
            })
        });
        if let Some((wire_index0, edge_index0)) = a {
            self[wire_index0].rotate_left(edge_index0);
            self[wire_index0].push_front(edge0.clone());
            self[wire_index0].push_back(edge0.inverse());
        }
        match (a, b) {
            (Some((wire_index0, edge_index0)), Some((wire_index1, edge_index1))) => {
                if wire_index0 == wire_index1 {
                    let len = self[wire_index0].len() - 2;
                    let edge_index1 = (len + edge_index1 - edge_index0) % len + 1;
                    let new_wire = self[wire_index0].split_off(edge_index1);
                    self[wire_index0].status = status;
                    self.push(BoundaryWire::new(new_wire, status.not()));
                } else {
                    let mut new_wire0 = self[wire_index1].clone();
                    let mut new_wire1 = new_wire0.split_off(edge_index1);
                    new_wire0.append(&mut self[wire_index0]);
                    new_wire0.append(&mut new_wire1);
                    self[wire_index0] = new_wire0;
                    self.swap_remove(wire_index1);
                }
            }
            (None, Some((wire_index1, edge_index1))) => {
                self[wire_index1].rotate_left(edge_index1);
                self[wire_index1].push_front(edge0.inverse());
                self[wire_index1].push_back(edge0);
            }
            (None, None) => self.push(BoundaryWire::new(
                vec![edge0.inverse(), edge0].into(),
                ShapesOpStatus::Unknown,
            )),
            _ => {}
        }
        [a, b]
    }
}

impl<P: Copy + Tolerance, C: Clone> LoopsStore<P, C> {
    #[inline(always)]
    fn change_vertex(
        &mut self,
        old_vertex: &Vertex<P>,
        new_vertex: &Vertex<P>,
        emap: &mut HashMap<EdgeID<C>, Edge<P, C>>,
    ) {
        self.iter_mut()
            .for_each(|loops| loops.change_vertex(old_vertex, new_vertex, emap));
    }

    #[inline(always)]
    fn swap_edge_into_wire(&mut self, edge_id: EdgeID<C>, new_wire: &Wire<P, C>) {
        self.iter_mut()
            .for_each(|loops| loops.swap_edge_into_wire(edge_id, new_wire))
    }

    fn add_polygon_vertex(
        &mut self,
        loops_index: usize,
        v: &Vertex<P>,
        emap: &mut HashMap<EdgeID<C>, Edge<P, C>>,
    ) -> Option<(usize, usize, ParameterKind)>
    where
        C: Cut<Point = P> + SearchParameter<D1, Point = P>,
    {
        let pt = v.point();
        let (wire_index, edge_index, kind) = self[loops_index].search_parameter(pt)?;
        match kind {
            ParameterKind::Front => {
                let old_vertex = self[loops_index][wire_index][edge_index]
                    .absolute_front()
                    .clone();
                self.change_vertex(&old_vertex, v, emap);
            }
            ParameterKind::Back => {
                let old_vertex = self[loops_index][wire_index][edge_index]
                    .absolute_back()
                    .clone();
                self.change_vertex(&old_vertex, v, emap);
            }
            ParameterKind::Inner(t) => {
                let edge = self[loops_index][wire_index][edge_index].absolute_clone();
                let edge_id = edge.id();
                let (edge0, edge1) = edge.cut_with_parameter(v, t)?;
                let new_wire: Wire<_, _> = vec![edge0, edge1].into();
                self.swap_edge_into_wire(edge_id, &new_wire);
            }
        }
        Some((wire_index, edge_index, kind))
    }
}

impl<C> LoopsStore<Point3, C> {
    fn add_geom_vertex<S>(
        &mut self,
        (loops_index, wire_index, edge_index): (usize, usize, usize),
        v: &Vertex<Point3>,
        kind: ParameterKind,
        another_surface: &S,
        emap: &mut HashMap<EdgeID<C>, Edge<Point3, C>>,
    ) -> Option<()>
    where
        C: Cut<Point = Point3, Vector = Vector3> + SearchNearestParameter<D1, Point = Point3>,
        S: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
    {
        match kind {
            ParameterKind::Front => {
                let old_vertex = self[loops_index][wire_index][edge_index]
                    .absolute_front()
                    .clone();
                v.set_point(old_vertex.point());
                self.change_vertex(&old_vertex, v, emap);
            }
            ParameterKind::Back => {
                let old_vertex = self[loops_index][wire_index][edge_index]
                    .absolute_back()
                    .clone();
                v.set_point(old_vertex.point());
                self.change_vertex(&old_vertex, v, emap);
            }
            ParameterKind::Inner(_) => {
                let curve = self[loops_index][wire_index][edge_index].curve();
                let (pt, t, _) =
                    curve_surface_projection(&curve, None, another_surface, None, v.point(), 100)?;
                let edge = self[loops_index][wire_index][edge_index].absolute_clone();
                let (front, back) = edge.curve().range_tuple();
                // PATCH(BrepSCAD): the projected parameter can land on, or a
                // rounding error outside, the curve's range when the
                // intersection endpoint coincides with an existing vertex.
                // Cutting there is rejected by `cut_with_parameter`, which
                // fails the whole boolean; snap to the existing vertex instead.
                if t <= front || front.near(&t) {
                    let old_vertex = edge.absolute_front().clone();
                    v.set_point(old_vertex.point());
                    self.change_vertex(&old_vertex, v, emap);
                } else if back <= t || back.near(&t) {
                    let old_vertex = edge.absolute_back().clone();
                    v.set_point(old_vertex.point());
                    self.change_vertex(&old_vertex, v, emap);
                } else {
                    v.set_point(pt);
                    let edge_id = edge.id();
                    let (edge0, edge1) = edge.cut_with_parameter(v, t)?;
                    let new_wire: Wire<_, _> = vec![edge0, edge1].into();
                    self.swap_edge_into_wire(edge_id, &new_wire);
                }
            }
        }
        Some(())
    }
}

fn curve_surface_projection<C, S>(
    curve: &C,
    curve_hint: Option<f64>,
    surface: &S,
    surface_hint: Option<(f64, f64)>,
    point: Point3,
    trials: usize,
) -> Option<(Point3, f64, Point2)>
where
    C: ParametricCurve3D + SearchNearestParameter<D1, Point = Point3>,
    S: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
{
    if trials == 0 {
        return None;
    }
    let t = curve.search_nearest_parameter(point, curve_hint, 10)?;
    let pt0 = curve.subs(t);
    let (u, v) = surface.search_nearest_parameter(point, surface_hint, 10)?;
    let pt1 = surface.subs(u, v);
    if point.near(&pt0) && point.near(&pt1) && pt0.near(&pt1) {
        Some((point, t, Point2::new(u, v)))
    } else {
        let l = curve.der(t);
        let n = surface.normal(u, v);
        let t0 = (pt1 - pt0).dot(n) / l.dot(n);
        curve_surface_projection(
            curve,
            Some(t),
            surface,
            Some((u, v)),
            pt0 + t0 * l,
            trials - 1,
        )
    }
}

fn create_independent_loop<P, C, D>(mut poly_curve0: C) -> Wire<P, D>
where
    C: Cut<Point = P>,
    D: From<C>, {
    let (t0, t1) = poly_curve0.range_tuple();
    let t = (t0 + t1) / 2.0;
    let poly_curve1 = poly_curve0.cut(t);
    let v0 = Vertex::new(poly_curve0.front());
    let v1 = Vertex::new(poly_curve1.front());
    let edge0 = Edge::new(&v0, &v1, poly_curve0.into());
    let edge1 = Edge::new(&v1, &v0, poly_curve1.into());
    wire![edge0, edge1]
}

#[allow(dead_code)]
pub struct LoopsStoreQuadruple<C> {
    pub geom_loops_store0: LoopsStore<Point3, C>,
    pub poly_loops_store0: LoopsStore<Point3, PolylineCurve>,
    pub geom_loops_store1: LoopsStore<Point3, C>,
    pub poly_loops_store1: LoopsStore<Point3, PolylineCurve>,
}

/// Cut one face of a coplanar pair along the overlap boundary.
///
/// Segments along the face's own boundary are dropped — the boundary is already
/// there — and the rest is attached to the face wire the way a transversal curve
/// is, with `add_polygon_vertex`/`add_geom_vertex` splicing the endpoints in and
/// splitting a face edge when a point lands in its middle.
///
/// Only straight, boundary-to-boundary cuts are sewn in. A curved one would need
/// an intersection curve that is genuinely parameterised on *both* surfaces, and
/// a coplanar pair has no such parameterisation: `divide_faces` walks the curve,
/// never converges, and the boolean hangs. Such pairs keep the ordinary path.
///
/// Returns the number of cuts sewn in, or `None` when one could not be attached.
/// Callers pass copies of the stores so a failed attachment is simply dropped.
fn cut_coplanar_face<C, S>(
    poly_store: &mut LoopsStore<Point3, PolylineCurve>,
    geom_store: &mut LoopsStore<Point3, C>,
    face_index: usize,
    polygon: &PolygonMesh,
    segments: &[(Point3, Point3)],
    own_surface: &S,
    other_surface: &S,
    status: ShapesOpStatus,
) -> Option<usize>
where
    C: Cut<Point = Point3, Vector = Vector3>
        + SearchNearestParameter<D1, Point = Point3>
        + From<IntersectionCurve<PolylineCurve, S, S>>,
    S: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3> + Clone,
{
    let boundary = intersection_curve::boundary_edges(polygon);
    let tolerance = 1.0e-7
        * segments
            .iter()
            .flat_map(|(start, end)| [start.x, start.y, start.z, end.x, end.y, end.z])
            .fold(1.0_f64, |scale, value| scale.max(value.abs()));

    let cuts: Vec<(Point3, Point3)> = segments
        .iter()
        .copied()
        .filter(|segment| !intersection_curve::segment_on_edges(*segment, &boundary, tolerance))
        .collect();

    let mut added = 0;
    for chain in chain_segments(&cuts, tolerance) {
        if chain.len() < 2 {
            continue;
        }
        let start = chain[0];
        let end = *chain.last()?;

        // Straight only: see the note above about curves with no two-surface
        // parameterisation.
        let straight = chain.windows(3).all(|window| {
            let first = window[1] - window[0];
            let second = window[2] - window[1];
            first.cross(second).magnitude() <= tolerance * first.magnitude().max(1.0)
        });
        if !straight {
            continue;
        }

        // A cut only means something when both ends reach the face's boundary:
        // that is where the shared region stops.
        if !intersection_curve::point_on_edges(start, &boundary, tolerance)
            || !intersection_curve::point_on_edges(end, &boundary, tolerance)
        {
            continue;
        }

        let polyline = PolylineCurve::from(chain.clone());
        let pv0 = Vertex::new(start);
        let pv1 = Vertex::new(end);
        let gv0 = Vertex::new(start);
        let gv1 = Vertex::new(end);
        let mut curve =
            IntersectionCurve::new(own_surface.clone(), other_surface.clone(), polyline.clone());

        let mut pemap = HashMap::default();
        let mut gemap = HashMap::default();
        let (wire_index, edge_index, kind) =
            poly_store.add_polygon_vertex(face_index, &pv0, &mut pemap)?;
        geom_store.add_geom_vertex(
            (face_index, wire_index, edge_index),
            &gv0,
            kind,
            other_surface,
            &mut gemap,
        )?;
        *curve.leader_mut().first_mut()? = gv0.point();

        let (wire_index, edge_index, kind) =
            poly_store.add_polygon_vertex(face_index, &pv1, &mut pemap)?;
        geom_store.add_geom_vertex(
            (face_index, wire_index, edge_index),
            &gv1,
            kind,
            other_surface,
            &mut gemap,
        )?;
        *curve.leader_mut().last_mut()? = gv1.point();

        poly_store[face_index].add_edge(Edge::new(&pv0, &pv1, polyline), status);
        geom_store[face_index].add_edge(Edge::new(&gv0, &gv1, curve.into()), status);
        added += 1;
    }
    Some(added)
}

/// Walk directed segments end to end into chains, growing from both ends: a run
/// of collinear segments can be produced in any order, and a forward-only walk
/// leaves the piece that should have preceded the first one stranded.
fn chain_segments(segments: &[(Point3, Point3)], tolerance: f64) -> Vec<Vec<Point3>> {
    let near = |a: Point3, b: Point3| (a - b).magnitude() < tolerance;
    let mut remaining: Vec<(Point3, Point3)> = segments.to_vec();
    let mut chains = Vec::new();

    while let Some(start) = remaining.pop() {
        let mut chain = vec![start.0, start.1];
        loop {
            let mut extended = false;
            let back = *chain.last().expect("chain is never empty");
            if let Some(index) = remaining
                .iter()
                .position(|(from, to)| near(*from, back) || near(*to, back))
            {
                let (from, to) = remaining.remove(index);
                chain.push(if near(from, back) { to } else { from });
                extended = true;
            }
            let front = chain[0];
            if let Some(index) = remaining
                .iter()
                .position(|(from, to)| near(*from, front) || near(*to, front))
            {
                let (from, to) = remaining.remove(index);
                chain.insert(0, if near(to, front) { from } else { to });
                extended = true;
            }
            if !extended {
                break;
            }
        }
        chains.push(chain);
    }
    chains
}

pub fn create_loops_stores<C, S>(
    geom_shell0: &Shell<Point3, C, S>,
    poly_shell0: &Shell<Point3, PolylineCurve, Option<PolygonMesh>>,
    geom_shell1: &Shell<Point3, C, S>,
    poly_shell1: &Shell<Point3, PolylineCurve, Option<PolygonMesh>>,
) -> Option<LoopsStoreQuadruple<C>>
where
    C: SearchNearestParameter<D1, Point = Point3>
        + SearchParameter<D1, Point = Point3>
        + Cut<Point = Point3, Vector = Vector3>
        + From<IntersectionCurve<PolylineCurve, S, S>>,
    S: ParametricSurface3D + SearchNearestParameter<D2, Point = Point3>,
{
    let mut geom_loops_store0: LoopsStore<_, _> = geom_shell0.face_iter().collect();
    let mut poly_loops_store0: LoopsStore<_, _> = poly_shell0.face_iter().collect();
    let mut geom_loops_store1: LoopsStore<_, _> = geom_shell1.face_iter().collect();
    let mut poly_loops_store1: LoopsStore<_, _> = poly_shell1.face_iter().collect();
    let store0_len = geom_loops_store0.len();
    let store1_len = geom_loops_store1.len();
    let mut coplanar_pairs: Vec<(usize, usize)> = Vec::new();
    (0..store0_len)
        .flat_map(move |i| (0..store1_len).map(move |j| (i, j)))
        .try_for_each(|(face_index0, face_index1)| {
            let ori0 = geom_shell0[face_index0].orientation();
            let ori1 = geom_shell1[face_index1].orientation();
            let surface0 = geom_shell0[face_index0].surface();
            let surface1 = geom_shell1[face_index1].surface();
            let polygon0 = poly_shell0[face_index0].surface()?;
            let polygon1 = poly_shell1[face_index1].surface()?;

            // PATCH(BrepSCAD): coplanar faces yield no interference curves, so
            // the patch they share is never cut out and the point-sample
            // classification then decides arbitrarily. The cuts are made in the
            // second pass below, once the transversal curves are attached:
            // splitting a face edge here would invalidate an endpoint another
            // curve still has to find.
            if intersection_curve::coplanar(&polygon0, &polygon1) {
                coplanar_pairs.push((face_index0, face_index1));
                return Some(());
            }

            intersection_curve::intersection_curves(
                surface0.clone(),
                &polygon0,
                surface1.clone(),
                &polygon1,
            )?
            .into_iter()
            .try_for_each(|(polyline, intersection_curve)| {
                let mut intersection_curve = intersection_curve.into();
                let status = ShapesOpStatus::from_is_curve(&intersection_curve)?;
                let (status0, status1) = match (ori0, ori1) {
                    (true, true) => (status, status.not()),
                    (true, false) => (status.not(), status.not()),
                    (false, true) => (status, status),
                    (false, false) => (status.not(), status),
                };
                if polyline.front().near(&polyline.back()) {
                    let poly_wire = create_independent_loop(polyline);
                    poly_loops_store0[face_index0]
                        .add_independent_loop(BoundaryWire::new(poly_wire.clone(), status0));
                    poly_loops_store1[face_index1]
                        .add_independent_loop(BoundaryWire::new(poly_wire, status1));
                    let geom_wire = create_independent_loop(intersection_curve);
                    geom_loops_store0[face_index0]
                        .add_independent_loop(BoundaryWire::new(geom_wire.clone(), status0));
                    geom_loops_store1[face_index1]
                        .add_independent_loop(BoundaryWire::new(geom_wire, status1));
                } else {
                    let pv0 = Vertex::new(polyline.front());
                    let pv1 = Vertex::new(polyline.back());
                    let gv0 = Vertex::new(polyline.front());
                    let gv1 = Vertex::new(polyline.back());
                    let mut pemap0 = HashMap::default();
                    let mut pemap1 = HashMap::default();
                    let mut gemap0 = HashMap::default();
                    let mut gemap1 = HashMap::default();
                    let idx00 =
                        poly_loops_store0.add_polygon_vertex(face_index0, &pv0, &mut pemap0);
                    if let Some((wire_index, edge_index, kind)) = idx00 {
                        geom_loops_store0.add_geom_vertex(
                            (face_index0, wire_index, edge_index),
                            &gv0,
                            kind,
                            &surface1,
                            &mut gemap0,
                        )?;
                        let polyline = intersection_curve.leader_mut();
                        *polyline.first_mut().unwrap() = gv0.point();
                    }
                    let idx01 =
                        poly_loops_store0.add_polygon_vertex(face_index0, &pv1, &mut pemap1);
                    if let Some((wire_index, edge_index, kind)) = idx01 {
                        geom_loops_store0.add_geom_vertex(
                            (face_index0, wire_index, edge_index),
                            &gv1,
                            kind,
                            &surface1,
                            &mut gemap1,
                        )?;
                        let polyline = intersection_curve.leader_mut();
                        *polyline.last_mut().unwrap() = gv1.point();
                    }
                    let idx10 =
                        poly_loops_store1.add_polygon_vertex(face_index1, &pv0, &mut pemap0);
                    if let Some((wire_index, edge_index, kind)) = idx10 {
                        geom_loops_store1.add_geom_vertex(
                            (face_index1, wire_index, edge_index),
                            &gv0,
                            kind,
                            &surface0,
                            &mut gemap0,
                        )?;
                        let polyline = intersection_curve.leader_mut();
                        *polyline.first_mut().unwrap() = gv0.point();
                    }
                    let idx11 =
                        poly_loops_store1.add_polygon_vertex(face_index1, &pv1, &mut pemap1);
                    if let Some((wire_index, edge_index, kind)) = idx11 {
                        geom_loops_store1.add_geom_vertex(
                            (face_index1, wire_index, edge_index),
                            &gv1,
                            kind,
                            &surface0,
                            &mut gemap1,
                        )?;
                        let polyline = intersection_curve.leader_mut();
                        *polyline.last_mut().unwrap() = gv1.point();
                    }
                    let pedge = Edge::new(&pv0, &pv1, polyline);
                    let gedge = Edge::new(&gv0, &gv1, intersection_curve.into());
                    poly_loops_store0[face_index0].add_edge(pedge.clone(), status0);
                    geom_loops_store0[face_index0].add_edge(gedge.clone(), status0);
                    poly_loops_store1[face_index1].add_edge(pedge, status1);
                    geom_loops_store1[face_index1].add_edge(gedge, status1);
                }
                Some(())
            })
        })?;
    // Second pass: cut the coplanar pairs once every transversal curve has been
    // attached. A cut that cannot be sewn into a face wire leaves the polygon and
    // geometric store out of step, so a pair's cuts are made on copies and kept
    // only when every chain attached.
    for (face_index0, face_index1) in coplanar_pairs {
        let ori0 = geom_shell0[face_index0].orientation();
        let ori1 = geom_shell1[face_index1].orientation();
        let surface0 = geom_shell0[face_index0].surface();
        let surface1 = geom_shell1[face_index1].surface();
        let (Some(polygon0), Some(polygon1)) = (
            poly_shell0[face_index0].surface(),
            poly_shell1[face_index1].surface(),
        ) else {
            continue;
        };

        let status = ShapesOpStatus::Or;
        let (status0, status1) = match (ori0, ori1) {
            (true, true) => (status, status.not()),
            (true, false) => (status.not(), status.not()),
            (false, true) => (status, status),
            (false, false) => (status.not(), status),
        };
        let segments = intersection_curve::coplanar_overlap_segments(&polygon0, &polygon1);

        let mut poly0 = poly_loops_store0.clone();
        let mut geom0 = geom_loops_store0.clone();
        let mut poly1 = poly_loops_store1.clone();
        let mut geom1 = geom_loops_store1.clone();
        let (Some(first), Some(second)) = (
            cut_coplanar_face(
                &mut poly0,
                &mut geom0,
                face_index0,
                &polygon0,
                &segments,
                &surface0,
                &surface1,
                status0,
            ),
            cut_coplanar_face(
                &mut poly1,
                &mut geom1,
                face_index1,
                &polygon1,
                &segments,
                &surface1,
                &surface0,
                status1,
            ),
        ) else {
            continue;
        };
        if first + second == 0 {
            continue;
        }
        poly_loops_store0 = poly0;
        geom_loops_store0 = geom0;
        poly_loops_store1 = poly1;
        geom_loops_store1 = geom1;
    }

    Some(LoopsStoreQuadruple {
        geom_loops_store0,
        poly_loops_store0,
        geom_loops_store1,
        poly_loops_store1,
    })
}

#[cfg(test)]
mod tests;
