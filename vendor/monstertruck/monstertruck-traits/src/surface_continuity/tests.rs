use super::*;

#[test]
fn checked_order_rejects_values_outside_the_public_range() {
    let error = ContinuityOrder::new(MAX_CONTINUITY_ORDER + 1)
        .expect_err("orders above G4 must be rejected");

    assert_eq!(error.requested(), 5);
    assert_eq!(error.maximum(), MAX_CONTINUITY_ORDER);
    assert!(!ContinuityOrder::G0.is_experimental());
    assert!(!ContinuityOrder::G3.is_experimental());
    assert!(ContinuityOrder::G4.is_experimental());
}

#[test]
fn checked_order_conversions_preserve_the_order() {
    [
        ContinuityOrder::G0,
        ContinuityOrder::G1,
        ContinuityOrder::G2,
        ContinuityOrder::G3,
        ContinuityOrder::G4,
    ]
    .into_iter()
    .enumerate()
    .for_each(|(order, checked)| {
        assert_eq!(ContinuityOrder::try_from(order), Ok(checked));
        assert_eq!(usize::from(checked), order);
        assert_eq!(checked.as_usize(), order);
    });
}

#[test]
fn capability_reports_preserve_every_side_and_typed_support() {
    [
        BoundarySide::MinU,
        BoundarySide::MaxU,
        BoundarySide::MinV,
        BoundarySide::MaxV,
    ]
    .into_iter()
    .for_each(|side| {
        let Ok(supported) = SurfaceContinuityCapability::try_supported_through(
            side,
            ContinuityOrder::G3,
            ContinuityOrder::G4,
        ) else {
            panic!("G4 must be a valid maximum for a G3 request");
        };
        let Ok(unsupported) = SurfaceContinuityCapability::try_unsupported(
            side,
            ContinuityOrder::G4,
            UnsupportedContinuityCapability::InsufficientDegree {
                available: 3,
                required: 4,
            },
            Some(ContinuityOrder::G3),
        ) else {
            panic!("G3 must be a valid maximum for an unsupported G4 request");
        };

        assert_eq!(supported.side(), side);
        assert_eq!(supported.requested(), ContinuityOrder::G3);
        assert_eq!(
            supported.maximum_supported_order(),
            Some(ContinuityOrder::G4)
        );
        assert_eq!(supported.unsupported_reason(), None);
        assert_eq!(unsupported.side(), side);
        assert_eq!(unsupported.requested(), ContinuityOrder::G4);
        assert_eq!(
            unsupported.maximum_supported_order(),
            Some(ContinuityOrder::G3)
        );
        assert_eq!(
            unsupported.unsupported_reason(),
            Some(UnsupportedContinuityCapability::InsufficientDegree {
                available: 3,
                required: 4,
            })
        );
    });
}

#[test]
fn capability_reports_reject_inconsistent_maximum_orders() {
    let supported = SurfaceContinuityCapability::try_supported_through(
        BoundarySide::MinU,
        ContinuityOrder::G3,
        ContinuityOrder::G2,
    )
    .expect_err("a supported maximum must cover the requested order");
    let unsupported = SurfaceContinuityCapability::try_unsupported(
        BoundarySide::MinU,
        ContinuityOrder::G3,
        UnsupportedContinuityCapability::InsufficientDegree {
            available: 2,
            required: 3,
        },
        Some(ContinuityOrder::G3),
    )
    .expect_err("an unsupported maximum must be below the requested order");

    assert_eq!(supported.requested(), ContinuityOrder::G3);
    assert_eq!(supported.maximum(), ContinuityOrder::G2);
    assert_eq!(unsupported.requested(), ContinuityOrder::G3);
    assert_eq!(unsupported.maximum(), ContinuityOrder::G3);
}
