//! Classic (0.3.2) boolean loops store.
//!
//! Ported verbatim from the published 0.3.2 crate's `transversal::loops_store`,
//! adjusted only for the graft: the marching intersection-curve backend now
//! yields a raw [`IntersectionCurve`] leader (the 0.3.2 parameter wrapper was
//! dropped), so the `.into()` that unwrapped the wrapper is gone.

#![allow(clippy::many_single_char_names)]

use super::intersection_curve;
use monstertruck_core::cgmath64::*;
use monstertruck_geometry::prelude::*;
use monstertruck_meshing::prelude::*;
use monstertruck_topology::{Vertex, *};
use rustc_hash::FxHashMap as HashMap;

type PolylineCurve = monstertruck_meshing::prelude::PolylineCurve<Point3>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ShapesOpStatus {
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
pub(super) struct BoundaryWire<P, C> {
    wire: Wire<P, C>,
    status: ShapesOpStatus,
    /// True when this wire came from the face's own boundary rather than from
    /// an intersection curve.
    boundary: bool,
}

impl<P, C> BoundaryWire<P, C> {
    #[inline(always)]
    pub(super) fn new(wire: Wire<P, C>, status: ShapesOpStatus) -> Self {
        Self {
            wire,
            status,
            boundary: false,
        }
    }
    #[inline(always)]
    pub(super) fn new_boundary(wire: Wire<P, C>) -> Self {
        Self {
            wire,
            status: ShapesOpStatus::Unknown,
            boundary: true,
        }
    }
    #[inline(always)]
    pub(super) fn split(wire: Wire<P, C>, status: ShapesOpStatus, boundary: bool) -> Self {
        Self {
            wire,
            status,
            boundary,
        }
    }
    #[inline(always)]
    pub(super) fn status(&self) -> ShapesOpStatus { self.status }
    #[inline(always)]
    pub(super) fn is_boundary(&self) -> bool { self.boundary }
    #[inline(always)]
    pub(super) fn inverse(&self) -> Self {
        Self {
            wire: self.wire.inverse(),
            status: self.status.not(),
            boundary: self.boundary,
        }
    }
}

impl ShapesOpStatus {
    fn from_is_curve<C, S0, S1>(curve: &IntersectionCurve<C, S0, S1>) -> Option<ShapesOpStatus>
    where
        C: ParametricCurve3D + BoundedCurve,
        S0: ParametricSurface3D + SearchNearestParameter<SurfaceParameter, Point = Point3>,
        S1: ParametricSurface3D + SearchNearestParameter<SurfaceParameter, Point = Point3>, {
        let (t0, t1) = curve.range_tuple();
        let t = (t0 + t1) / 2.0;
        let (_, pt0, pt1) = curve.search_triple(t, 100)?;
        let der = curve.leader().derivative(t);
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
pub(super) struct Loops<P, C>(Vec<BoundaryWire<P, C>>);
#[derive(Clone, Debug)]
pub(super) struct LoopsStore<P, C>(Vec<Loops<P, C>>);

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
            .map(|wire| BoundaryWire::new_boundary(wire.clone()))
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

/// What the geometry side did with a vertex, so the polygon side can follow it.
///
/// The two stores used to classify the same vertex independently - once from the
/// polyline parameter, once from the exact curve's projection - and disagreed
/// wherever the polyline and the curve differ enough to move the parameter across
/// a range end. The polygon store then cut an edge the geometry store had only
/// snapped to, and the next index past that point was off by one. The geometry
/// side decides now, and this says what it decided.
#[derive(Clone, Debug, Copy, PartialEq)]
enum Placement {
    /// Snapped to the edge's front vertex, no cut.
    Front,
    /// Snapped to the edge's back vertex, no cut.
    Back,
    /// Cut the edge in two at the given parameter.
    Cut(f64),
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
    where C: BoundedCurve<Point = P> + SearchParameter<CurveParameter, Point = P> {
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
        emap: &mut HashMap<EdgeId<C>, Edge<P, C>>,
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
                // Remove the edge from the HashMap when it is no longer there because Id reassignment will occur.
                if edge.count() == 1 {
                    emap.remove(&edge.id());
                }
                *edge = new_edge;
            })
    }

    fn swap_edge_into_wire(&mut self, edge_id: EdgeId<C>, new_wire: &Wire<P, C>) {
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
                    let boundary = self[wire_index0].boundary;
                    self.push(BoundaryWire::split(new_wire, status.not(), boundary));
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
        emap: &mut HashMap<EdgeId<C>, Edge<P, C>>,
    ) {
        self.iter_mut()
            .for_each(|loops| loops.change_vertex(old_vertex, new_vertex, emap));
    }

    #[inline(always)]
    fn swap_edge_into_wire(&mut self, edge_id: EdgeId<C>, new_wire: &Wire<P, C>) {
        self.iter_mut()
            .for_each(|loops| loops.swap_edge_into_wire(edge_id, new_wire))
    }

    /// Finds where a vertex belongs, without changing the store.
    ///
    /// `add_polygon_vertex` used to decide and mutate in one step, which cut the
    /// polygon wire before the geometry store had been told about the same
    /// vertex. The two layouts then disagreed about the edge indices and the
    /// geometry side indexed past its own store. Deciding first lets both stores
    /// be updated from the same indices.
    fn find_polygon_vertex(
        &self,
        loops_index: usize,
        pt: P,
        tol: f64,
    ) -> Option<(usize, usize, ParameterKind)>
    where
        P: MetricSpace<Metric = f64>,
        C: BoundedCurve<Point = P> + SearchParameter<CurveParameter, Point = P>,
    {
        let (wire_index, edge_index, kind) = self[loops_index].search_parameter(pt)?;
        if let ParameterKind::Inner(t) = kind {
            // `search_parameter` returns the nearest parameter with no distance
            // bound, so a vertex well off the edge would be accepted here.
            let curve = self[loops_index][wire_index][edge_index].curve();
            if curve.evaluate(t).distance(pt) > tol {
                return None;
            }
        }
        Some((wire_index, edge_index, kind))
    }

    /// Applies the placement the geometry side decided.
    ///
    /// `kind` is this store's own classification of the vertex and `placement` is
    /// what the geometry store did with it. They can disagree: the geometry side
    /// re-derives the parameter from the exact curve and snaps to an existing
    /// vertex when it lands at a range end, while this store classified from the
    /// polyline. Following `placement` keeps the two layouts the same shape, so a
    /// cut on one side is a cut on the other.
    fn apply_polygon_vertex(
        &mut self,
        loops_index: usize,
        v: &Vertex<P>,
        emap: &mut HashMap<EdgeId<C>, Edge<P, C>>,
        (wire_index, edge_index, kind): (usize, usize, ParameterKind),
        placement: Placement,
    ) -> Option<()>
    where
        C: Cut<Point = P> + SearchParameter<CurveParameter, Point = P>,
    {
        match placement {
            Placement::Front => {
                let old_vertex = self[loops_index][wire_index][edge_index]
                    .absolute_front()
                    .clone();
                self.change_vertex(&old_vertex, v, emap);
            }
            Placement::Back => {
                let old_vertex = self[loops_index][wire_index][edge_index]
                    .absolute_back()
                    .clone();
                self.change_vertex(&old_vertex, v, emap);
            }
            Placement::Cut(_) => {
                // Cut at this store's own parameter: the polyline and the curve
                // do not share a parameterisation, so the geometry side's `t`
                // would land somewhere else entirely.
                let ParameterKind::Inner(t) = kind else {
                    return None;
                };
                let edge = self[loops_index][wire_index][edge_index].absolute_clone();
                let edge_id = edge.id();
                let (edge0, edge1) = edge.cut_with_parameter(v, t)?;
                let new_wire: Wire<_, _> = vec![edge0, edge1].into();
                self.swap_edge_into_wire(edge_id, &new_wire);
            }
        }
        Some(())
    }
}

impl<C> LoopsStore<Point3, C> {
    fn add_geom_vertex<S>(
        &mut self,
        (loops_index, wire_index, edge_index): (usize, usize, usize),
        v: &Vertex<Point3>,
        kind: ParameterKind,
        another_surface: &S,
        emap: &mut HashMap<EdgeId<C>, Edge<Point3, C>>,
        tol: f64,
    ) -> Option<Placement>
    where
        C: Cut<Point = Point3, Vector = Vector3>
            + SearchNearestParameter<CurveParameter, Point = Point3>,
        S: ParametricSurface3D + SearchNearestParameter<SurfaceParameter, Point = Point3>,
    {
        // PATCH(BrepSCAD): the indices come from the *polygon* store's
        // `search_parameter`, but they are used to index the *geometry* store.
        // The two layouts diverge as soon as one has had an edge cut and the
        // other has not, and indexing then panics - which reaches the caller as a
        // crash rather than the clean failure the retry ladder expects.
        if loops_index >= self.len()
            || wire_index >= self[loops_index].len()
            || edge_index >= self[loops_index][wire_index].len()
        {
            if std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some() {
                eprintln!(
                    "TRACE loops: index out of range loops={loops_index}/{} wire={wire_index}/{} edge={edge_index}",
                    self.len(),
                    match loops_index < self.len() {
                        true => self[loops_index].len(),
                        false => 0,
                    }
                );
            }
            return None;
        }
        let placement = match kind {
            ParameterKind::Front => {
                let old_vertex = self[loops_index][wire_index][edge_index]
                    .absolute_front()
                    .clone();
                v.set_point(old_vertex.point());
                self.change_vertex(&old_vertex, v, emap);
                Placement::Front
            }
            ParameterKind::Back => {
                let old_vertex = self[loops_index][wire_index][edge_index]
                    .absolute_back()
                    .clone();
                v.set_point(old_vertex.point());
                self.change_vertex(&old_vertex, v, emap);
                Placement::Back
            }
            ParameterKind::Inner(_) => {
                let curve = self[loops_index][wire_index][edge_index].curve();
                let projection =
                    curve_surface_projection(&curve, None, another_surface, None, v.point(), 100);
                // PATCH(BrepSCAD): the fixed-point projection cannot converge when
                // the edge runs nearly tangent to the other surface, which is what
                // a knife edge meeting a wall looks like. The vertex is already a
                // point of the intersection curve, so it lies on both surfaces and
                // the nearest parameter on the edge is enough to place it. Failing
                // the whole boolean over a refinement step is far worse than
                // placing the vertex to within `SEARCH_TOLERANCE`.
                let (pt, t) = match projection {
                    Some((pt, t, _)) => (pt, t),
                    None => {
                        let t = curve.search_nearest_parameter(v.point(), None, 100)?;
                        let pt = curve.evaluate(t);
                        let offset = pt.distance(v.point());
                        // The gap between the polyline and the exact curve reaches
                        // 0.073 on the snap, which is 1.5x the modelling tolerance.
                        // The vertex came from the intersection curve, so snapping
                        // it onto the edge is closer to right than discarding the
                        // pair and every face in it, but it is still bounded so a
                        // vertex belonging to some other edge cannot be dragged in.
                        if offset > tol * 2.0 {
                            if std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some() {
                                eprintln!(
                                    "TRACE loops: projection failed and the nearest point is {offset:.6} away on face {loops_index} wire {wire_index} edge {edge_index}"
                                );
                            }
                            return None;
                        }
                        (pt, t)
                    }
                };
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
                    Placement::Front
                } else if back <= t || back.near(&t) {
                    let old_vertex = edge.absolute_back().clone();
                    v.set_point(old_vertex.point());
                    self.change_vertex(&old_vertex, v, emap);
                    Placement::Back
                } else {
                    v.set_point(pt);
                    let edge_id = edge.id();
                    let (edge0, edge1) = edge.cut_with_parameter(v, t)?;
                    let new_wire: Wire<_, _> = vec![edge0, edge1].into();
                    self.swap_edge_into_wire(edge_id, &new_wire);
                    Placement::Cut(t)
                }
            }
        };
        Some(placement)
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
    C: ParametricCurve3D + SearchNearestParameter<CurveParameter, Point = Point3>,
    S: ParametricSurface3D + SearchNearestParameter<SurfaceParameter, Point = Point3>,
{
    // PATCH(BrepSCAD): iterate instead of recursing. Every retry carries the
    // generic surface/curve search frames, and the 100 retries of a degenerate
    // intersection overflowed an 8 MB stack (opengrid-snap.scad). The loop is
    // the same fixed-point iteration: `trials` attempts, newest hint each time.
    let mut curve_hint = curve_hint;
    let mut surface_hint = surface_hint;
    let mut point = point;
    for _ in 0..trials {
        let t = curve.search_nearest_parameter(point, curve_hint, 10)?;
        let pt0 = curve.evaluate(t);
        let (u, v) = surface.search_nearest_parameter(point, surface_hint, 10)?;
        let pt1 = surface.evaluate(u, v);
        if point.near(&pt0) && point.near(&pt1) && pt0.near(&pt1) {
            return Some((point, t, Point2::new(u, v)));
        }
        let l = curve.derivative(t);
        let n = surface.normal(u, v);
        let denominator = l.dot(n);
        // A curve tangent to the surface has no transverse direction to step in.
        // Without this the step becomes `inf` and every later hint is `nan`, so
        // the iteration cannot recover - which is how a knife edge meeting a wall
        // loses the whole boolean.
        if denominator.abs() <= f64::EPSILON {
            return None;
        }
        let t0 = (pt1 - pt0).dot(n) / denominator;
        curve_hint = Some(t);
        surface_hint = Some((u, v));
        point = pt0 + t0 * l;
    }
    None
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
pub(super) struct LoopsStoreQuadruple<C> {
    pub(super) geom_loops_store0: LoopsStore<Point3, C>,
    pub(super) poly_loops_store0: LoopsStore<Point3, PolylineCurve>,
    pub(super) geom_loops_store1: LoopsStore<Point3, C>,
    pub(super) poly_loops_store1: LoopsStore<Point3, PolylineCurve>,
}

pub(super) fn create_loops_stores<C, S>(
    geom_shell0: &Shell<Point3, C, S>,
    poly_shell0: &Shell<Point3, PolylineCurve, Option<PolygonMesh>>,
    geom_shell1: &Shell<Point3, C, S>,
    poly_shell1: &Shell<Point3, PolylineCurve, Option<PolygonMesh>>,
    flat: Option<f64>,
    tol: f64,
) -> Option<LoopsStoreQuadruple<C>>
where
    C: SearchNearestParameter<CurveParameter, Point = Point3>
        + SearchParameter<CurveParameter, Point = Point3>
        + ParameterDivision1D<Point = Point3>
        + Cut<Point = Point3, Vector = Vector3>
        + From<IntersectionCurve<PolylineCurve, S, S>>
        + Send
        + Sync,
    S: ParametricSurface3D
        + Send
        + Sync
        + Clone
        + TryIntoAnalyticSurfaceKind
        + SearchParameter<SurfaceParameter, Point = Point3>
        + SearchNearestParameter<SurfaceParameter, Point = Point3>,
{
    let mut geom_loops_store0: LoopsStore<_, _> = geom_shell0.face_iter().collect();
    let mut poly_loops_store0: LoopsStore<_, _> = poly_shell0.face_iter().collect();
    let mut geom_loops_store1: LoopsStore<_, _> = geom_shell1.face_iter().collect();
    let mut poly_loops_store1: LoopsStore<_, _> = poly_shell1.face_iter().collect();
    let store0_len = geom_loops_store0.len();
    let store1_len = geom_loops_store1.len();
    if std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some() {
        for (label, shell) in [("shell0", poly_shell0), ("shell1", poly_shell1)] {
            for (index, face) in shell.face_iter().enumerate() {
                let mut min = [f64::INFINITY; 3];
                let mut max = [f64::NEG_INFINITY; 3];
                for vertex in face.vertex_iter() {
                    let point = vertex.point();
                    for (axis, value) in [point.x, point.y, point.z].into_iter().enumerate() {
                        min[axis] = min[axis].min(value);
                        max[axis] = max[axis].max(value);
                    }
                }
                eprintln!("TRACE face/{label}[{index}]: {min:.3?}..{max:.3?}");
            }
        }
    }
    let bounds = |shell: &Shell<Point3, PolylineCurve, Option<PolygonMesh>>| -> Vec<Option<BoundingBox<Point3>>> {
        shell
            .face_iter()
            .map(|face| face.surface().map(|mesh| mesh.positions().iter().collect()))
            .collect()
    };
    let (bounds0, bounds1) = (bounds(poly_shell0), bounds(poly_shell1));
    let touching = |a: &BoundingBox<Point3>, b: &BoundingBox<Point3>| {
        let margin = (a.diameter() + b.diameter()) * 1.0e-6 + TOLERANCE;
        let (a0, a1, b0, b1) = (a.min(), a.max(), b.min(), b.max());
        (0..3).all(|k| a0[k] <= b1[k] + margin && b0[k] <= a1[k] + margin)
    };
    let planes0: Vec<_> = geom_shell0
        .face_iter()
        .map(super::coplanar::plane_of)
        .collect();
    let planes1: Vec<_> = geom_shell1
        .face_iter()
        .map(super::coplanar::plane_of)
        .collect();
    let pairs: Vec<(usize, usize)> = (0..store0_len)
        .flat_map(|i| (0..store1_len).map(move |j| (i, j)))
        .filter(|&(i, j)| match (&bounds0[i], &bounds1[j]) {
            (Some(a), Some(b)) => touching(a, b),
            _ => true,
        })
        .filter(|&(i, j)| match (flat, planes0[i], planes1[j]) {
            (Some(tol), Some(a), Some(b)) => !super::coplanar::coplanar(a, b, tol),
            _ => true,
        })
        .collect();
    let along_boundary =
        |face: &Face<Point3, PolylineCurve, Option<PolygonMesh>>, p: Point3, tol: f64| {
            face.edge_iter().any(|edge| {
                let poly = edge.curve();
                poly.windows(2).any(|s| {
                    let ab = s[1] - s[0];
                    let t = ((p - s[0]).dot(ab) / ab.magnitude2().max(1.0e-300)).clamp(0.0, 1.0);
                    (s[0] + ab * t).distance(p) < tol
                })
            })
        };
    let curves_of = |&(face_index0, face_index1): &(usize, usize)| {
        let traced = std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some();
        let polygon0 = match poly_shell0[face_index0].surface() {
            Some(polygon) => polygon,
            None => {
                if traced {
                    eprintln!("TRACE loops: shell0 face {face_index0} has no polygon");
                }
                return None;
            }
        };
        let polygon1 = match poly_shell1[face_index1].surface() {
            Some(polygon) => polygon,
            None => {
                if traced {
                    eprintln!("TRACE loops: shell1 face {face_index1} has no polygon");
                }
                return None;
            }
        };
        let surface0 = geom_shell0[face_index0].surface();
        let surface1 = geom_shell1[face_index1].surface();
        let planar = planes0[face_index0].is_some() && planes1[face_index1].is_some();
        if traced {
            eprintln!(
                "TRACE loops: pair {face_index0} x {face_index1} interference={} tris={}x{} planar={planar}",
                polygon0.extract_interference(&polygon1).len(),
                polygon0.faces().triangle_iter().count(),
                polygon1.faces().triangle_iter().count()
            );
        }
        let mut curves = match intersection_curve::intersection_curves(
            surface0.clone(),
            &polygon0,
            surface1.clone(),
            &polygon1,
        ) {
            Some(curves) => curves,
            None if planar => Vec::new(),
            None => {
                if traced {
                    eprintln!(
                        "TRACE loops: no intersection curve for {face_index0} x {face_index1} (planar={planar})"
                    );
                }
                return None;
            }
        };
        if let Some(tol) = flat {
            curves = curves
                .into_iter()
                .filter(|(polyline, _)| {
                    !polyline.windows(2).all(|pair| {
                        (0..=4).all(|k| {
                            let p = pair[0] + (pair[1] - pair[0]) * (k as f64 / 4.0);
                            along_boundary(&poly_shell0[face_index0], p, tol)
                                && along_boundary(&poly_shell1[face_index1], p, tol)
                        })
                    })
                })
                .collect();
        }
        if curves.is_empty() && traced {
            eprintln!(
                "TRACE loops: pair {face_index0} x {face_index1} found no intersection curve (planar={planar})"
            );
        }
        if traced && !curves.is_empty() {
            eprintln!(
                "TRACE loops: pair {face_index0} x {face_index1} found {} curve(s) (planar={planar})",
                curves.len()
            );
        }
        if curves.is_empty() && planar {
            for points in super::coplanar::planar_intersections(
                &geom_shell0[face_index0],
                &geom_shell1[face_index1],
                tol,
            )
            .unwrap_or_default()
            {
                if points.len() < 2 {
                    continue;
                }
                let polyline = PolylineCurve(points);
                let curve =
                    IntersectionCurve::new(surface0.clone(), surface1.clone(), polyline.clone());
                curves.push((polyline, curve));
            }
        }
        Some(curves)
    };
    #[cfg(not(target_arch = "wasm32"))]
    let found: Vec<_> = {
        use rayon::prelude::*;
        pairs
            .par_iter()
            .map(curves_of)
            .collect::<Option<Vec<_>>>()?
    };
    #[cfg(target_arch = "wasm32")]
    let found: Vec<_> = pairs.iter().map(curves_of).collect::<Option<Vec<_>>>()?;
    pairs
        .into_iter()
        .zip(found)
        .try_for_each(|((face_index0, face_index1), curves)| {
            let ori0 = geom_shell0[face_index0].orientation();
            let ori1 = geom_shell1[face_index1].orientation();
            let surface0 = geom_shell0[face_index0].surface();
            let surface1 = geom_shell1[face_index1].surface();
            curves
                .into_iter()
                .try_for_each(|(polyline, mut intersection_curve)| {
                    let status = match ShapesOpStatus::from_is_curve(&intersection_curve) {
                        Some(status) => status,
                        None => {
                            if std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some() {
                                eprintln!(
                                    "TRACE loops: no status for the curve between {face_index0} and {face_index1}"
                                );
                            }
                            return None;
                        }
                    };
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
                        if let Some(found) =
                            poly_loops_store0.find_polygon_vertex(face_index0, pv0.point(), tol)
                        {
                            let placement = geom_loops_store0.add_geom_vertex(
                                (face_index0, found.0, found.1),
                                &gv0,
                                found.2,
                                &surface1,
                                &mut gemap0,
                                tol,
                            )?;
                            poly_loops_store0.apply_polygon_vertex(face_index0, &pv0, &mut pemap0, found, placement)?;
                            let polyline = intersection_curve.leader_mut();
                            *polyline.first_mut().unwrap() = gv0.point();
                        }
                        if let Some(found) =
                            poly_loops_store0.find_polygon_vertex(face_index0, pv1.point(), tol)
                        {
                            let placement = geom_loops_store0.add_geom_vertex(
                                (face_index0, found.0, found.1),
                                &gv1,
                                found.2,
                                &surface1,
                                &mut gemap1,
                                tol,
                            )?;
                            poly_loops_store0.apply_polygon_vertex(face_index0, &pv1, &mut pemap1, found, placement)?;
                            let polyline = intersection_curve.leader_mut();
                            *polyline.last_mut().unwrap() = gv1.point();
                        }
                        if let Some(found) =
                            poly_loops_store1.find_polygon_vertex(face_index1, pv0.point(), tol)
                        {
                            let placement = geom_loops_store1.add_geom_vertex(
                                (face_index1, found.0, found.1),
                                &gv0,
                                found.2,
                                &surface0,
                                &mut gemap0,
                                tol,
                            )?;
                            poly_loops_store1.apply_polygon_vertex(face_index1, &pv0, &mut pemap0, found, placement)?;
                            let polyline = intersection_curve.leader_mut();
                            *polyline.first_mut().unwrap() = gv0.point();
                        }
                        if let Some(found) =
                            poly_loops_store1.find_polygon_vertex(face_index1, pv1.point(), tol)
                        {
                            let placement = geom_loops_store1.add_geom_vertex(
                                (face_index1, found.0, found.1),
                                &gv1,
                                found.2,
                                &surface0,
                                &mut gemap1,
                                tol,
                            )?;
                            poly_loops_store1.apply_polygon_vertex(face_index1, &pv1, &mut pemap1, found, placement)?;
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
    if std::env::var_os("BREPSCAD_DEBUG_TRACE").is_some() {
        for (label, store) in [
            ("shell0", &poly_loops_store0),
            ("shell1", &poly_loops_store1),
        ] {
            for (index, loops) in store.iter().enumerate() {
                let wires = loops
                    .iter()
                    .map(|wire| format!("{}e/{:?}", wire.len(), wire.status()))
                    .collect::<Vec<_>>()
                    .join(" ");
                eprintln!("TRACE loops store/{label}[{index}]: {wires}");
            }
        }
    }
    Some(LoopsStoreQuadruple {
        geom_loops_store0,
        poly_loops_store0,
        geom_loops_store1,
        poly_loops_store1,
    })
}
