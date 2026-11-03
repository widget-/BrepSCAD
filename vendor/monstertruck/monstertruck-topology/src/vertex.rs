use crate::*;

impl<P> Vertex<P> {
    /// constructor
    /// # Examples
    /// ```
    /// use monstertruck_topology::*;
    /// let v0 = Vertex::new(()); // a vertex whose geometry is the empty tuple.
    /// let v1 = Vertex::new(()); // another vertex
    /// let v2 = v0.clone(); // a cloned vertex
    /// assert_ne!(v0, v1);
    /// assert_eq!(v0, v2);
    /// ```
    #[inline(always)]
    pub fn new(point: P) -> Vertex<P> {
        Vertex {
            point: Arc::new(Mutex::new(point)),
            stable_id: StableId::UNASSIGNED,
        }
    }

    /// Creates a vertex with an explicit [`StableId`].
    #[inline(always)]
    pub fn new_with_id(point: P, stable_id: StableId) -> Vertex<P> {
        Vertex {
            point: Arc::new(Mutex::new(point)),
            stable_id,
        }
    }

    /// Returns the stable persistent identifier of this vertex.
    #[inline(always)]
    pub fn stable_id(&self) -> StableId { self.stable_id }

    /// Sets the stable persistent identifier of this vertex.
    #[inline(always)]
    pub fn set_stable_id(&mut self, id: StableId) { self.stable_id = id; }

    /// Creates distinct vertices from points.
    /// # Examples
    /// ```
    /// use monstertruck_topology::*;
    /// let v = Vertex::from_points(&[(), (), ()]);
    /// assert_eq!(v.len(), 3);
    /// assert_ne!(v[0], v[2]);
    /// ```
    #[inline(always)]
    pub fn from_points(points: impl AsRef<[P]>) -> Vec<Vertex<P>>
    where P: Copy {
        points.as_ref().iter().map(|p| Vertex::new(*p)).collect()
    }

    /// Returns the point of vertex.
    #[inline(always)]
    pub fn point(&self) -> P
    where P: Clone {
        self.point.lock().clone()
    }

    /// Sets the point of vertex.
    /// # Examples
    /// ```
    /// use monstertruck_topology::*;
    /// let v0 = Vertex::new(0);
    /// let v1 = v0.clone();
    ///
    /// // Two vertices have the same content.
    /// assert_eq!(v0.point(), 0);
    /// assert_eq!(v1.point(), 0);
    ///
    /// // set point
    /// v0.set_point(1);
    ///
    /// // The contents of two vertices are synchronized.
    /// assert_eq!(v0.point(), 1);
    /// assert_eq!(v1.point(), 1);
    /// ```
    #[inline(always)]
    pub fn set_point(&self, point: P) { *self.point.lock() = point; }

    /// Returns vertex whose point is converted by `point_mapping`.
    /// # Remarks
    /// Accessing geometry elements directly in the closure will result in a deadlock.
    /// So, this method does not appear to the document.
    #[doc(hidden)]
    #[inline(always)]
    pub fn try_mapped<Q>(
        &self,
        mut point_mapping: impl FnMut(&P) -> Option<Q>,
    ) -> Option<Vertex<Q>> {
        Some(Vertex::new_with_id(
            point_mapping(&*self.point.lock())?,
            self.stable_id,
        ))
    }

    /// Returns vertex whose point is converted by `point_mapping`.
    /// # Examples
    /// ```
    /// use monstertruck_topology::*;
    /// let v0 = Vertex::new(2);
    /// let v1 = v0.mapped(|a| *a as f64 + 0.5);
    /// assert_eq!(v1.point(), 2.5);
    /// ```
    /// # Remarks
    /// Accessing geometry elements directly in the closure will result in a deadlock.
    /// So, this method does not appear to the document.
    #[doc(hidden)]
    #[inline(always)]
    pub fn mapped<Q>(&self, mut point_mapping: impl FnMut(&P) -> Q) -> Vertex<Q> {
        Vertex::new_with_id(point_mapping(&*self.point.lock()), self.stable_id)
    }

    /// Returns the id of the vertex.
    #[inline(always)]
    pub fn id(&self) -> VertexId<P> { Id::new(Arc::as_ptr(&self.point)) }

    /// Returns how many same vertices.
    ///
    /// # Examples
    /// ```
    /// use monstertruck_topology::*;
    /// // Create one vertex
    /// let v0 = Vertex::new(());
    /// assert_eq!(v0.count(), 1);
    /// // Create another vertex, independent from v0
    /// let v1 = Vertex::new(());
    /// assert_eq!(v0.count(), 1);
    /// // Clone v0, count will be 2
    /// let v2 = v0.clone();
    /// assert_eq!(v0.count(), 2);
    /// assert_eq!(v2.count(), 2);
    /// // drop v2, count will be 1
    /// drop(v2);
    /// assert_eq!(v0.count(), 1);
    /// ```
    #[inline(always)]
    pub fn count(&self) -> usize { Arc::strong_count(&self.point) }

    /// Create display struct for debugging the vertex.
    /// # Examples
    /// ```
    /// use monstertruck_topology::*;
    /// use VertexDisplayFormat as VDF;
    /// let v = Vertex::new([0, 2]);
    /// assert_eq!(
    ///     format!("{:?}", v.display(VDF::Full)),
    ///     format!("Vertex {{ id: {:?}, entity: [0, 2] }}", v.id()),
    /// );
    /// assert_eq!(
    ///     format!("{:?}", v.display(VDF::IdTuple)),
    ///     format!("Vertex({:?})", v.id()),
    /// );
    /// assert_eq!(
    ///     &format!("{:?}", v.display(VDF::PointTuple)),
    ///     "Vertex([0, 2])",
    /// );
    /// assert_eq!(
    ///     &format!("{:?}", v.display(VDF::AsPoint)),
    ///     "[0, 2]",
    /// );
    /// ```
    #[inline(always)]
    pub fn display(
        &self,
        format: VertexDisplayFormat,
    ) -> DebugDisplay<'_, Self, VertexDisplayFormat> {
        DebugDisplay {
            entity: self,
            format,
        }
    }
}

impl<P> Clone for Vertex<P> {
    #[inline(always)]
    fn clone(&self) -> Vertex<P> {
        Vertex {
            point: Arc::clone(&self.point),
            stable_id: self.stable_id,
        }
    }
}

impl<P> PartialEq for Vertex<P> {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool { self.id() == other.id() }
}

impl<P> Eq for Vertex<P> {}

impl<P> Hash for Vertex<P> {
    #[inline(always)]
    fn hash<H: Hasher>(&self, state: &mut H) { std::ptr::hash(Arc::as_ptr(&self.point), state); }
}

impl<P: Debug> Debug for DebugDisplay<'_, Vertex<P>, VertexDisplayFormat> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self.format {
            VertexDisplayFormat::Full => f
                .debug_struct("Vertex")
                .field("id", &Arc::as_ptr(&self.entity.point))
                .field("entity", &MutexFmt(&self.entity.point))
                .finish(),
            VertexDisplayFormat::IdTuple => {
                f.debug_tuple("Vertex").field(&self.entity.id()).finish()
            }
            VertexDisplayFormat::PointTuple => f
                .debug_tuple("Vertex")
                .field(&MutexFmt(&self.entity.point))
                .finish(),
            VertexDisplayFormat::AsPoint => {
                f.write_fmt(format_args!("{:?}", MutexFmt(&self.entity.point)))
            }
        }
    }
}

#[test]
fn vertex_stable_id_survives_clone() {
    let mut alloc = StableIdAllocator::new();
    let v = Vertex::new_with_id((), alloc.allocate());
    let v2 = v.clone();
    assert_eq!(v.stable_id(), v2.stable_id());
    assert!(v.stable_id().is_assigned());
}

#[test]
fn vertex_default_stable_id_is_unassigned() {
    let v = Vertex::new(());
    assert!(!v.stable_id().is_assigned());
    assert_eq!(v.stable_id(), StableId::UNASSIGNED);
}

#[test]
fn edge_stable_id_survives_inverse() {
    let v = Vertex::from_points([(), ()]);
    let mut edge = Edge::new(&v[0], &v[1], ());
    edge.set_stable_id(StableId::new(42));
    let inv = edge.inverse();
    assert_eq!(edge.stable_id(), inv.stable_id());
}

#[test]
fn face_stable_id_survives_clone() {
    use crate::Wire;
    let v = Vertex::from_points([(), (), ()]);
    let wire = Wire::from(vec![
        Edge::new(&v[0], &v[1], ()),
        Edge::new(&v[1], &v[2], ()),
        Edge::new(&v[2], &v[0], ()),
    ]);
    let mut face = Face::new(vec![wire], ());
    face.set_stable_id(StableId::new(99));
    let face2 = face.clone();
    assert_eq!(face.stable_id(), face2.stable_id());
}

#[test]
fn solid_alloc_id() {
    let v = Vertex::from_points([(); 8]);
    let edge = [
        Edge::new(&v[0], &v[1], ()),
        Edge::new(&v[1], &v[2], ()),
        Edge::new(&v[2], &v[3], ()),
        Edge::new(&v[3], &v[0], ()),
        Edge::new(&v[0], &v[4], ()),
        Edge::new(&v[1], &v[5], ()),
        Edge::new(&v[2], &v[6], ()),
        Edge::new(&v[3], &v[7], ()),
        Edge::new(&v[4], &v[5], ()),
        Edge::new(&v[5], &v[6], ()),
        Edge::new(&v[6], &v[7], ()),
        Edge::new(&v[7], &v[4], ()),
    ];
    let wire0 = wire![&edge[0], &edge[1], &edge[2], &edge[3]];
    let wire1 = wire![&edge[4], &edge[8], &edge[5].inverse(), &edge[0].inverse()];
    let wire2 = wire![&edge[5], &edge[9], &edge[6].inverse(), &edge[1].inverse()];
    let wire3 = wire![&edge[6], &edge[10], &edge[7].inverse(), &edge[2].inverse()];
    let wire4 = wire![&edge[7], &edge[11], &edge[4].inverse(), &edge[3].inverse()];
    let wire5 = wire![
        &edge[11].inverse(),
        &edge[10].inverse(),
        &edge[9].inverse(),
        &edge[8].inverse(),
    ];
    let face0 = Face::new(vec![wire0], ());
    let face1 = Face::new(vec![wire1], ());
    let face2 = Face::new(vec![wire2], ());
    let face3 = Face::new(vec![wire3], ());
    let face4 = Face::new(vec![wire4], ());
    let face5 = Face::new(vec![wire5], ());
    let shell: Shell<(), (), ()> = vec![face0, face1, face2, face3, face4, face5].into();
    let mut solid = Solid::new(vec![shell]);
    let a = solid.alloc_id();
    let b = solid.alloc_id();
    assert_ne!(a, b);
    assert_eq!(a.raw(), 1);
    assert_eq!(b.raw(), 2);
}
