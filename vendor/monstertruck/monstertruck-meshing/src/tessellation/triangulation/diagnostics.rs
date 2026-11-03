//! Loud, digest-safe diagnostics for silently dropped faces.
//!
//! When a face fails to tessellate it is emitted with `surface = None` (or an
//! empty mesh). Historically this was *silent*: the meshed shell was quietly
//! missing a face and its volume was understated, with no signal at all (spec
//! 006 corner-100 revolve-pole cap -> mesh `None` -> volume read flat; spec 007
//! C3 periodic tube seam). This module is the tessellator-side analogue of the
//! kernel's typed-refusal doctrine: it makes every drop *visible* without
//! changing any geometry.
//!
//! Three layered, always-safe channels:
//!
//! * a process-global counter ([`face_drop_count`]) -- a queryable metric any
//!   caller can poll to learn that the emitted mesh is incomplete,
//! * a structured [`log::warn!`] naming the dropped face, its surface class and
//!   the reason (visible to any logger-equipped consumer -- viewer, GPU,
//!   renderer), and
//! * an `MT_FACE_DROP`- / `MT_MESH_TRACE`-gated `eprintln!` census line for
//!   offline blast-radius measurement.
//!
//! This module emits *observability only*. The mesh, the volume, and every
//! downstream boolean result are byte-identical whether or not a drop is
//! reported: the polygon is classified through a shared borrow and returned
//! unchanged. Escalating a drop to a hard/typed error is deliberately **out of
//! scope** here (it changes outcomes and needs a user decision -- see D1b).

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};

/// Why a face produced no usable mesh (was silently dropped).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FaceDropReason {
    /// Untrimmed face whose surface reports no bounded `(u, v)` domain, so the
    /// fast untrimmed path cannot mesh it (`try_range_tuple` returned `None`).
    UnboundedDomain,
    /// A trimmed boundary could not be projected into surface parameter space
    /// (`PolyBoundaryPiece::try_new*` returned `None` for at least one loop):
    /// a boundary vertex would not project onto the surface. This is the
    /// revolve-pole / periodic-seam / degenerate-trim family.
    BoundaryProjectionFailed,
    /// A mesh was produced but is empty (zero faces): the face contributes
    /// nothing to the output shell yet reports no failure.
    EmptyTessellation,
}

impl FaceDropReason {
    /// Stable machine-readable tag for census parsing and structured log fields.
    pub const fn tag(self) -> &'static str {
        match self {
            Self::UnboundedDomain => "unbounded-domain",
            Self::BoundaryProjectionFailed => "boundary-projection-failed",
            Self::EmptyTessellation => "empty-tessellation",
        }
    }
}

impl std::fmt::Display for FaceDropReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str(self.tag()) }
}

static FACE_DROP_COUNT: AtomicU64 = AtomicU64::new(0);

/// Number of faces the tessellator has silently dropped in this process since
/// start (or since the last [`reset_face_drop_count`]). A nonzero value means
/// at least one emitted shell is missing a face and its volume is understated.
pub fn face_drop_count() -> u64 { FACE_DROP_COUNT.load(Ordering::Relaxed) }

/// Resets the global face-drop counter to zero and returns its previous value.
/// Intended for tests and per-run blast-radius measurement.
pub fn reset_face_drop_count() -> u64 { FACE_DROP_COUNT.swap(0, Ordering::Relaxed) }

/// Whether the machine-readable census `eprintln!` line is enabled. Gated so
/// normal runs stay quiet; set `MT_FACE_DROP=1` (or the broader `MT_MESH_TRACE`)
/// to capture drops for a census.
fn census_enabled() -> bool { env::var_os("MT_FACE_DROP").is_some() || mesh_trace_enabled() }

/// Classifies a completed face polygon into a drop reason, or `None` if the face
/// meshed successfully. This is the single source of truth for "is this a
/// drop"; it is pure and side-effect free so it can be unit tested directly.
///
/// `pub(crate)` so the opt-in strict meshing path
/// ([`crate::tessellation::shell_to_polygon_strict`]) classifies a just-meshed
/// shell's faces with the *same* logic the always-on warn floor uses.
pub(crate) fn classify_face_drop(
    polygon: Option<&PolygonMesh>,
    is_untrimmed: bool,
) -> Option<FaceDropReason> {
    match polygon {
        None if is_untrimmed => Some(FaceDropReason::UnboundedDomain),
        None => Some(FaceDropReason::BoundaryProjectionFailed),
        Some(mesh) if mesh.faces().is_empty() => Some(FaceDropReason::EmptyTessellation),
        Some(_) => None,
    }
}

/// Observes a just-computed face polygon and, if it is a silent drop, records it
/// on all three channels (counter + `log::warn!` + gated census line).
///
/// Pure side effect: `polygon` is inspected through a shared borrow and is never
/// mutated, so the emitted mesh/volume is identical with and without this call.
pub(super) fn observe_face_drop<S: PreMeshableSurface>(
    surface: &S,
    face_idx: Option<usize>,
    polygon: Option<&PolygonMesh>,
    is_untrimmed: bool,
    loops: usize,
    edges: usize,
) {
    let Some(reason) = classify_face_drop(polygon, is_untrimmed) else {
        return;
    };
    FACE_DROP_COUNT.fetch_add(1, Ordering::Relaxed);
    let face = face_idx.map_or(-1i64, |index| index as i64);
    let surface_type = std::any::type_name::<S>();
    let periodic_u = surface.period_u().is_some();
    let periodic_v = surface.period_v().is_some();
    log::warn!(
        "tessellation dropped face {face} ({reason}): surface={surface_type} \
         loops={loops} edges={edges} periodic_u={periodic_u} periodic_v={periodic_v} \
         -- the meshed shell is missing this face and its volume is understated",
    );
    if census_enabled() {
        eprintln!(
            "FACE_DROP face={face} reason={reason} surface={surface_type} \
             loops={loops} edges={edges} periodic_u={periodic_u} periodic_v={periodic_v}",
        );
    }
}
