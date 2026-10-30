//! Truck primitive builders.

use truck_modeling::*;
use visual_openscad_kernel_api::KernelError;

type KernelResult<T> = std::result::Result<T, KernelError>;

pub(crate) fn cuboid(size: [f64; 3]) -> KernelResult<Solid> {
    if size.iter().any(|value| *value <= 0.0) {
        return Err(KernelError::Backend(
            "cuboid dimensions must be positive".to_owned(),
        ));
    }

    let vertex = builder::vertex(Point3::new(-size[0] / 2.0, -size[1] / 2.0, -size[2] / 2.0));
    let edge = builder::tsweep(&vertex, Vector3::unit_x() * size[0]);
    let face = builder::tsweep(&edge, Vector3::unit_y() * size[1]);
    Ok(builder::tsweep(&face, Vector3::unit_z() * size[2]))
}

pub(crate) fn square(size: [f64; 2]) -> KernelResult<Face> {
    if size.iter().any(|value| *value <= 0.0) {
        return Err(KernelError::Backend(
            "square dimensions must be positive".to_owned(),
        ));
    }
    let [width, height] = size;
    polygon(&[
        [-width / 2.0, -height / 2.0],
        [width / 2.0, -height / 2.0],
        [width / 2.0, height / 2.0],
        [-width / 2.0, height / 2.0],
    ])
}

pub(crate) fn circle(radius: f64) -> KernelResult<Face> {
    if radius <= 0.0 {
        return Err(KernelError::Backend(
            "circle radius must be positive".to_owned(),
        ));
    }

    let vertex = builder::vertex(Point3::new(radius, 0.0, 0.0));
    let wire = builder::rsweep(
        &vertex,
        Point3::origin(),
        Vector3::unit_z(),
        Rad(std::f64::consts::TAU),
    );
    builder::try_attach_plane(&[wire])
        .map_err(|error| KernelError::Backend(format!("circle failed: {error:?}")))
}

pub(crate) fn polygon(points: &[[f64; 2]]) -> KernelResult<Face> {
    if points.len() < 3 {
        return Err(KernelError::Backend(
            "polygon needs at least three points".to_owned(),
        ));
    }
    if points.iter().flatten().any(|value| !value.is_finite()) {
        return Err(KernelError::Backend(
            "polygon points must be finite".to_owned(),
        ));
    }

    let area = signed_area(points);
    if area.abs() < f64::EPSILON {
        return Err(KernelError::Backend("polygon has zero area".to_owned()));
    }

    let ordered: Vec<[f64; 2]> = if area < 0.0 {
        points.iter().rev().copied().collect()
    } else {
        points.to_vec()
    };
    let vertices: Vec<Vertex> = ordered
        .iter()
        .map(|[x, y]| builder::vertex(Point3::new(*x, *y, 0.0)))
        .collect();
    let edges: Vec<Edge> = (0..vertices.len())
        .map(|index| builder::line(&vertices[index], &vertices[(index + 1) % vertices.len()]))
        .collect();
    let wire: Wire = edges.into_iter().collect();
    builder::try_attach_plane(&[wire])
        .map_err(|error| KernelError::Backend(format!("polygon failed: {error:?}")))
}

fn signed_area(points: &[[f64; 2]]) -> f64 {
    let mut area = 0.0;
    for index in 0..points.len() {
        let [x0, y0] = points[index];
        let [x1, y1] = points[(index + 1) % points.len()];
        area += x0 * y1 - x1 * y0;
    }
    area / 2.0
}

pub(crate) fn cylinder(radius: f64, height: f64) -> KernelResult<Solid> {
    if radius <= 0.0 || height <= 0.0 {
        return Err(KernelError::Backend(
            "cylinder dimensions must be positive".to_owned(),
        ));
    }

    let vertex = builder::vertex(Point3::new(radius, 0.0, -height / 2.0));
    let circle = builder::rsweep(
        &vertex,
        Point3::origin(),
        Vector3::unit_z(),
        Rad(std::f64::consts::TAU),
    );
    let disk = builder::try_attach_plane(&[circle])
        .map_err(|error| KernelError::Backend(format!("{error:?}")))?;
    Ok(builder::tsweep(&disk, Vector3::new(0.0, 0.0, height)))
}

const SPHERE_SEGMENTS: usize = 32;
const SPHERE_RINGS: usize = 16;

pub(crate) fn sphere(radius: f64) -> KernelResult<Solid> {
    if radius <= 0.0 {
        return Err(KernelError::Backend(
            "sphere radius must be positive".to_owned(),
        ));
    }

    let segments = SPHERE_SEGMENTS;
    let rings = SPHERE_RINGS;
    let north = builder::vertex(Point3::new(0.0, 0.0, radius));
    let south = builder::vertex(Point3::new(0.0, 0.0, -radius));

    let mut latitude = Vec::with_capacity(rings - 1);
    for ring in 1..rings {
        let theta = std::f64::consts::PI * ring as f64 / rings as f64;
        let (sin_theta, cos_theta) = theta.sin_cos();
        let mut row = Vec::with_capacity(segments);
        for segment in 0..segments {
            let phi = std::f64::consts::TAU * segment as f64 / segments as f64;
            let (sin_phi, cos_phi) = phi.sin_cos();
            row.push(builder::vertex(Point3::new(
                radius * sin_theta * cos_phi,
                radius * sin_theta * sin_phi,
                radius * cos_theta,
            )));
        }
        latitude.push(row);
    }

    let mut horizontal = Vec::with_capacity(latitude.len());
    for row in &latitude {
        let mut edges = Vec::with_capacity(segments);
        for segment in 0..segments {
            edges.push(builder::line(&row[segment], &row[(segment + 1) % segments]));
        }
        horizontal.push(edges);
    }

    let mut vertical = Vec::with_capacity(latitude.len().saturating_sub(1));
    let mut diagonal = Vec::with_capacity(latitude.len().saturating_sub(1));
    for ring in 0..latitude.len().saturating_sub(1) {
        let mut vertical_edges = Vec::with_capacity(segments);
        let mut diagonal_edges = Vec::with_capacity(segments);
        for segment in 0..segments {
            vertical_edges.push(builder::line(
                &latitude[ring][segment],
                &latitude[ring + 1][segment],
            ));
            diagonal_edges.push(builder::line(
                &latitude[ring][segment],
                &latitude[ring + 1][(segment + 1) % segments],
            ));
        }
        vertical.push(vertical_edges);
        diagonal.push(diagonal_edges);
    }

    let mut north_edges = Vec::with_capacity(segments);
    let mut south_edges = Vec::with_capacity(segments);
    let last_ring = latitude.len() - 1;
    for (north_vertex, south_vertex) in latitude[0].iter().zip(&latitude[last_ring]) {
        north_edges.push(builder::line(&north, north_vertex));
        south_edges.push(builder::line(&south, south_vertex));
    }

    let mut faces = Vec::with_capacity(2 * segments * latitude.len());
    for segment in 0..segments {
        let wire: Wire = vec![
            north_edges[segment].clone(),
            horizontal[0][segment].clone(),
            north_edges[(segment + 1) % segments].inverse(),
        ]
        .into_iter()
        .collect();
        faces.push(
            builder::try_attach_plane(&[wire])
                .map_err(|error| KernelError::Backend(format!("sphere cap failed: {error:?}")))?,
        );

        let wire: Wire = vec![
            south_edges[(segment + 1) % segments].clone(),
            horizontal[last_ring][segment].inverse(),
            south_edges[segment].inverse(),
        ]
        .into_iter()
        .collect();
        faces.push(
            builder::try_attach_plane(&[wire])
                .map_err(|error| KernelError::Backend(format!("sphere cap failed: {error:?}")))?,
        );
    }

    for ring in 0..vertical.len() {
        for segment in 0..segments {
            let wire: Wire = vec![
                vertical[ring][segment].clone(),
                horizontal[ring + 1][segment].clone(),
                diagonal[ring][segment].inverse(),
            ]
            .into_iter()
            .collect();
            faces.push(builder::try_attach_plane(&[wire]).map_err(|error| {
                KernelError::Backend(format!("sphere facet failed: {error:?}"))
            })?);

            let wire: Wire = vec![
                diagonal[ring][segment].clone(),
                vertical[ring][(segment + 1) % segments].inverse(),
                horizontal[ring][segment].inverse(),
            ]
            .into_iter()
            .collect();
            faces.push(builder::try_attach_plane(&[wire]).map_err(|error| {
                KernelError::Backend(format!("sphere facet failed: {error:?}"))
            })?);
        }
    }

    let shell: Shell = faces.into_iter().collect();
    Solid::try_new(vec![shell])
        .map_err(|error| KernelError::Backend(format!("sphere shell failed: {error:?}")))
}
