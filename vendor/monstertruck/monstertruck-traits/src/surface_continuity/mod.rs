//! Checked geometric-continuity requests and surface capability diagnostics.
//!
//! These types are scalar-neutral trait foundations. Numerical solvers can use
//! them without making continuity order or boundary-side vocabulary specific
//! to one geometry representation.

use thiserror::Error;

/// Highest continuity order currently represented by the public API.
pub const MAX_CONTINUITY_ORDER: usize = 4;

/// A requested continuity order is outside the represented range.
#[derive(Clone, Copy, Debug, Error, Hash, PartialEq, Eq)]
#[error("continuity order {requested} exceeds the supported maximum {maximum}")]
pub struct UnsupportedContinuityOrder {
    requested: usize,
    maximum: usize,
}

impl UnsupportedContinuityOrder {
    /// Returns the rejected order.
    pub const fn requested(self) -> usize { self.requested }

    /// Returns the highest represented order.
    pub const fn maximum(self) -> usize { self.maximum }
}

/// Checked geometric-continuity order.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContinuityOrder(u8);

impl ContinuityOrder {
    /// Positional continuity.
    pub const G0: Self = Self(0);
    /// Tangent-plane continuity.
    pub const G1: Self = Self(1);
    /// Curvature continuity.
    pub const G2: Self = Self(2);
    /// Third-order geometric continuity.
    pub const G3: Self = Self(3);
    /// Experimental fourth-order geometric continuity.
    pub const G4: Self = Self(4);

    /// Creates a checked continuity order.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedContinuityOrder`] when `order` exceeds
    /// [`MAX_CONTINUITY_ORDER`].
    pub const fn new(order: usize) -> Result<Self, UnsupportedContinuityOrder> {
        if order <= MAX_CONTINUITY_ORDER {
            Ok(Self(order as u8))
        } else {
            Err(UnsupportedContinuityOrder {
                requested: order,
                maximum: MAX_CONTINUITY_ORDER,
            })
        }
    }

    /// Returns the numeric derivative order.
    #[inline(always)]
    pub const fn as_usize(self) -> usize { self.0 as usize }

    /// Returns whether this order is experimental.
    #[inline(always)]
    pub const fn is_experimental(self) -> bool { self.0 == Self::G4.0 }
}

impl TryFrom<usize> for ContinuityOrder {
    type Error = UnsupportedContinuityOrder;

    fn try_from(value: usize) -> Result<Self, Self::Error> { Self::new(value) }
}

impl From<ContinuityOrder> for usize {
    fn from(value: ContinuityOrder) -> Self { value.as_usize() }
}

/// Side of a full tensor-product surface parameter domain.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum BoundarySide {
    /// Side on which `u` is minimal.
    MinU,
    /// Side on which `u` is maximal.
    MaxU,
    /// Side on which `v` is minimal.
    MinV,
    /// Side on which `v` is maximal.
    MaxV,
}

/// Typed reason that a representation cannot support a continuity request.
#[derive(Clone, Copy, Debug, Error, Hash, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnsupportedContinuityCapability {
    /// The cross-boundary degree is below the mathematical minimum.
    #[error("cross-boundary degree {available} is below the required degree {required}")]
    InsufficientDegree {
        /// Available cross-boundary degree.
        available: usize,
        /// Required cross-boundary degree.
        required: usize,
    },
    /// The control net has too few cross-boundary rows for the requested jet.
    #[error("{available} cross-boundary control rows are below the required {required}")]
    InsufficientControlRows {
        /// Available cross-boundary control rows.
        available: usize,
        /// Required cross-boundary control rows.
        required: usize,
    },
    /// Both degree and control-row requirements are unsatisfied.
    #[error(
        "cross-boundary degree {available_degree} and control rows {available_rows} are below the required {required_degree} and {required_rows}"
    )]
    InsufficientDegreeAndControlRows {
        /// Available cross-boundary degree.
        available_degree: usize,
        /// Required cross-boundary degree.
        required_degree: usize,
        /// Available cross-boundary control rows.
        available_rows: usize,
        /// Required cross-boundary control rows.
        required_rows: usize,
    },
    /// The representation has an invalid control net.
    #[error("the surface control net is invalid")]
    InvalidControlNet,
    /// The representation has an invalid knot vector.
    #[error("the surface knot vector is invalid")]
    InvalidKnotVector,
    /// The inspected boundary is not clamped.
    #[error("the inspected boundary is not clamped")]
    UnclampedBoundary,
    /// A rational representation contains a non-finite weight.
    #[error("the surface contains a non-finite rational weight")]
    NonFiniteWeight,
    /// A rational representation contains a non-positive weight.
    #[error("the surface contains a non-positive rational weight")]
    NonPositiveWeight,
    /// The inspected side belongs to a periodic parameter direction.
    #[error("the inspected boundary is periodic")]
    PeriodicBoundary,
    /// The surface representation cannot expose the requested boundary jet.
    #[error("the surface representation cannot expose the requested boundary jet")]
    UnsupportedRepresentation,
    /// The request addresses a trimmed seam rather than a full patch side.
    #[error("trimmed seams are not supported by full-side continuity capability")]
    TrimmedBoundary,
}

/// A capability report declared an inconsistent maximum order.
#[derive(Clone, Copy, Debug, Error, Hash, PartialEq, Eq)]
#[error(
    "maximum supported continuity order {maximum:?} is inconsistent with the requested order {requested:?}"
)]
pub struct InvalidContinuityCapability {
    requested: ContinuityOrder,
    maximum: ContinuityOrder,
}

impl InvalidContinuityCapability {
    /// Returns the requested continuity order.
    pub const fn requested(self) -> ContinuityOrder { self.requested }

    /// Returns the inconsistent maximum order.
    pub const fn maximum(self) -> ContinuityOrder { self.maximum }
}

/// Typed support determination for one continuity capability report.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
#[non_exhaustive]
pub enum SurfaceContinuitySupport {
    /// The request is supported through the reported maximum order.
    Supported {
        /// Highest supported continuity order.
        maximum_order: ContinuityOrder,
    },
    /// The request is unsupported for a typed reason.
    Unsupported {
        /// Actionable unsupported condition.
        reason: UnsupportedContinuityCapability,
        /// Highest supported order, when capability inspection succeeded.
        maximum_order: Option<ContinuityOrder>,
    },
}

/// A representation-specific capability report for one full surface side.
///
/// Concrete surface implementations determine support using their own degree,
/// knot, control-net, and representation requirements. This report carries
/// that determination without embedding those rules in the trait crate. It
/// does not establish compatibility with another surface or feasibility for a
/// numerical solver. A report for `G4` remains experimental.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub struct SurfaceContinuityCapability {
    side: BoundarySide,
    requested: ContinuityOrder,
    support: SurfaceContinuitySupport,
}

impl SurfaceContinuityCapability {
    /// Reports support through an explicitly inspected maximum order.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidContinuityCapability`] when `maximum_order` is below
    /// `requested`.
    pub const fn try_supported_through(
        side: BoundarySide,
        requested: ContinuityOrder,
        maximum_order: ContinuityOrder,
    ) -> Result<Self, InvalidContinuityCapability> {
        if maximum_order.as_usize() < requested.as_usize() {
            Err(InvalidContinuityCapability {
                requested,
                maximum: maximum_order,
            })
        } else {
            Ok(Self {
                side,
                requested,
                support: SurfaceContinuitySupport::Supported { maximum_order },
            })
        }
    }

    /// Reports a typed unsupported condition and any known maximum order.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidContinuityCapability`] when `maximum_order` is not
    /// below `requested`.
    pub const fn try_unsupported(
        side: BoundarySide,
        requested: ContinuityOrder,
        reason: UnsupportedContinuityCapability,
        maximum_order: Option<ContinuityOrder>,
    ) -> Result<Self, InvalidContinuityCapability> {
        match maximum_order {
            Some(maximum) if maximum.as_usize() >= requested.as_usize() => {
                Err(InvalidContinuityCapability { requested, maximum })
            }
            _ => Ok(Self {
                side,
                requested,
                support: SurfaceContinuitySupport::Unsupported {
                    reason,
                    maximum_order,
                },
            }),
        }
    }

    /// Returns the inspected surface side.
    pub const fn side(self) -> BoundarySide { self.side }

    /// Returns the requested continuity order.
    pub const fn requested(self) -> ContinuityOrder { self.requested }

    /// Returns the typed support determination.
    pub const fn support(self) -> SurfaceContinuitySupport { self.support }

    /// Returns the highest supported order when it is known.
    pub const fn maximum_supported_order(self) -> Option<ContinuityOrder> {
        match self.support {
            SurfaceContinuitySupport::Supported { maximum_order } => Some(maximum_order),
            SurfaceContinuitySupport::Unsupported { maximum_order, .. } => maximum_order,
        }
    }

    /// Returns the typed reason that the request is unsupported.
    pub const fn unsupported_reason(self) -> Option<UnsupportedContinuityCapability> {
        match self.support {
            SurfaceContinuitySupport::Supported { .. } => None,
            SurfaceContinuitySupport::Unsupported { reason, .. } => Some(reason),
        }
    }

    /// Requires the representation to support the request.
    ///
    /// # Errors
    ///
    /// Returns [`UnsupportedContinuityCapability`] with the specific failed
    /// representation requirement.
    pub const fn require_supported(self) -> Result<Self, UnsupportedContinuityCapability> {
        match self.support {
            SurfaceContinuitySupport::Supported { .. } => Ok(self),
            SurfaceContinuitySupport::Unsupported { reason, .. } => Err(reason),
        }
    }
}

#[cfg(test)]
mod tests;
