/// Evaluates the cubic B-spline basis function at parameter `u` for knot vector `a` (length 5).
///
/// The basis function is a piecewise cubic polynomial with 4 segments over `[a[0], a[4])`.
/// Returns 0 outside this range.
pub(super) fn basis_function(u: f64, a: &[f64]) -> f64 {
    if u < a[0] || a[4] <= u {
        return 0.0;
    }

    if u < a[1] {
        let d = u - a[0];
        d * d * d / ((a[3] - a[0]) * (a[2] - a[0]) * (a[1] - a[0]))
    } else if u < a[2] {
        let scalar = 1.0 / (a[2] - a[1]);
        let terms: [(usize, usize, usize, usize, usize, usize, usize); 3] = [
            (0, 0, 2, 3, 0, 2, 0),
            (0, 1, 3, 3, 0, 3, 1),
            (1, 1, 4, 4, 1, 3, 1),
        ];
        scalar
            * terms
                .iter()
                .fold(0.0, |sum, &(n0, n1, n2, d0, d1, d2, d3)| {
                    sum + ((u - a[n0]) * (u - a[n1]) * (a[n2] - u))
                        / ((a[d0] - a[d1]) * (a[d2] - a[d3]))
                })
    } else if u < a[3] {
        let scalar = 1.0 / (a[3] - a[2]);
        let terms: [(usize, usize, usize, usize, usize, usize, usize); 3] = [
            (0, 3, 3, 3, 0, 3, 1),
            (1, 4, 3, 4, 1, 3, 1),
            (2, 4, 4, 4, 1, 4, 2),
        ];
        scalar
            * terms
                .iter()
                .fold(0.0, |sum, &(n0, n1, n2, d0, d1, d2, d3)| {
                    sum + ((u - a[n0]) * (a[n1] - u) * (a[n2] - u))
                        / ((a[d0] - a[d1]) * (a[d2] - a[d3]))
                })
    } else {
        let d = a[4] - u;
        d * d * d / ((a[4] - a[1]) * (a[4] - a[2]) * (a[4] - a[3]))
    }
}

/// Evaluates the 1st derivative of the cubic B-spline basis function at parameter `u`.
fn basis_function_d1(u: f64, a: &[f64]) -> f64 {
    if u < a[0] || a[4] <= u {
        return 0.0;
    }

    if u < a[1] {
        // N = (u-a0)^3 / D => N' = 3(u-a0)^2 / D.
        let d = u - a[0];
        3.0 * d * d / ((a[3] - a[0]) * (a[2] - a[0]) * (a[1] - a[0]))
    } else if u < a[2] {
        // Segment 2: sum of terms (u-ai)(u-aj)(ak-u) / denom, times scalar.
        // Each term is a product of 3 linear factors; derivative via product rule.
        let scalar = 1.0 / (a[2] - a[1]);
        let terms: [(usize, usize, usize, usize, usize, usize, usize); 3] = [
            (0, 0, 2, 3, 0, 2, 0),
            (0, 1, 3, 3, 0, 3, 1),
            (1, 1, 4, 4, 1, 3, 1),
        ];
        scalar
            * terms
                .iter()
                .fold(0.0, |sum, &(n0, n1, n2, d0, d1, d2, d3)| {
                    // f = (u-a[n0])(u-a[n1])(a[n2]-u), f' = product rule.
                    let f0 = u - a[n0];
                    let f1 = u - a[n1];
                    let f2 = a[n2] - u;
                    let denom = (a[d0] - a[d1]) * (a[d2] - a[d3]);
                    sum + (f1 * f2 + f0 * f2 - f0 * f1) / denom
                })
    } else if u < a[3] {
        let scalar = 1.0 / (a[3] - a[2]);
        let terms: [(usize, usize, usize, usize, usize, usize, usize); 3] = [
            (0, 3, 3, 3, 0, 3, 1),
            (1, 4, 3, 4, 1, 3, 1),
            (2, 4, 4, 4, 1, 4, 2),
        ];
        scalar
            * terms
                .iter()
                .fold(0.0, |sum, &(n0, n1, n2, d0, d1, d2, d3)| {
                    // f = (u-a[n0])(a[n1]-u)(a[n2]-u), f' = product rule.
                    let f0 = u - a[n0];
                    let f1 = a[n1] - u;
                    let f2 = a[n2] - u;
                    let denom = (a[d0] - a[d1]) * (a[d2] - a[d3]);
                    sum + (f1 * f2 - f0 * f2 - f0 * f1) / denom
                })
    } else {
        // N = (a4-u)^3 / D => N' = -3(a4-u)^2 / D.
        let d = a[4] - u;
        -3.0 * d * d / ((a[4] - a[1]) * (a[4] - a[2]) * (a[4] - a[3]))
    }
}

/// Evaluates the 2nd derivative of the cubic B-spline basis function at parameter `u`.
fn basis_function_d2(u: f64, a: &[f64]) -> f64 {
    if u < a[0] || a[4] <= u {
        return 0.0;
    }

    if u < a[1] {
        // N'' = 6(u-a0) / D.
        6.0 * (u - a[0]) / ((a[3] - a[0]) * (a[2] - a[0]) * (a[1] - a[0]))
    } else if u < a[2] {
        let scalar = 1.0 / (a[2] - a[1]);
        let terms: [(usize, usize, usize, usize, usize, usize, usize); 3] = [
            (0, 0, 2, 3, 0, 2, 0),
            (0, 1, 3, 3, 0, 3, 1),
            (1, 1, 4, 4, 1, 3, 1),
        ];
        scalar
            * terms
                .iter()
                .fold(0.0, |sum, &(n0, n1, n2, d0, d1, d2, d3)| {
                    // f = (u-a[n0])(u-a[n1])(a[n2]-u).
                    // f'' = 2[(a[n2]-u) - (u-a[n0]) - (u-a[n1])].
                    // Expanding: f'' = 2(a[n2] + a[n0] + a[n1] - 3u).
                    let denom = (a[d0] - a[d1]) * (a[d2] - a[d3]);
                    sum + 2.0 * (a[n2] + a[n0] + a[n1] - 3.0 * u) / denom
                })
    } else if u < a[3] {
        let scalar = 1.0 / (a[3] - a[2]);
        let terms: [(usize, usize, usize, usize, usize, usize, usize); 3] = [
            (0, 3, 3, 3, 0, 3, 1),
            (1, 4, 3, 4, 1, 3, 1),
            (2, 4, 4, 4, 1, 4, 2),
        ];
        scalar
            * terms
                .iter()
                .fold(0.0, |sum, &(n0, n1, n2, d0, d1, d2, d3)| {
                    // f = (u-a[n0])(a[n1]-u)(a[n2]-u).
                    // f' = f1*f2 - f0*f2 - f0*f1.
                    // f'' = -2f2 - 2f1 + 2f0 = 2(f0 - f1 - f2).
                    // f'' = 2((u-a[n0]) - (a[n1]-u) - (a[n2]-u)) = 2(3u - a[n0] - a[n1] - a[n2]).
                    let denom = (a[d0] - a[d1]) * (a[d2] - a[d3]);
                    sum + 2.0 * (3.0 * u - a[n0] - a[n1] - a[n2]) / denom
                })
    } else {
        // N'' = 6(a4-u) / D.
        6.0 * (a[4] - u) / ((a[4] - a[1]) * (a[4] - a[2]) * (a[4] - a[3]))
    }
}

/// Step size for central finite-difference derivative approximation (fallback for orders > 2).
pub(super) const DIFF_EPS: f64 = 1.0e-6;

/// Selects the appropriate basis function evaluator for the given derivative order.
pub(super) fn basis_function_der(u: f64, a: &[f64], der_order: usize) -> f64 {
    match der_order {
        0 => basis_function(u, a),
        1 => basis_function_d1(u, a),
        2 => basis_function_d2(u, a),
        _ => {
            // Fall back to finite differences for orders > 2.
            let h = DIFF_EPS;
            (basis_function_der(u + h, a, der_order - 1)
                - basis_function_der(u - h, a, der_order - 1))
                / (2.0 * h)
        }
    }
}
