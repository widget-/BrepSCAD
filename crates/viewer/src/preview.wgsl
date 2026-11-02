// Compute preview: one ray per pixel against the packed B-rep scene.
//
// This mirrors `brepscad_raycast` (planes, cylinders, cones, balls,
// B-spline/NURBS patches, even-odd trim loops, median-split BVH) in single
// precision. The CPU caster stays the reference: if the two disagree, the CPU
// one is right.

const KIND_PLANE: u32 = 0u;
const KIND_CYLINDER: u32 = 1u;
const KIND_CONE: u32 = 2u;
const KIND_TRIANGLES: u32 = 3u;
const KIND_SPHERE: u32 = 4u;
const KIND_SPLINE: u32 = 5u;
const MAX_DEGREE: u32 = 8u;

const TAU: f32 = 6.283185307179586;
const FAR: f32 = 1.0e30;
const STACK_DEPTH: u32 = 64u;

struct GpuNode {
    minimum: vec4<f32>,
    maximum: vec4<f32>,
    links: vec4<u32>, // left child, right child, first face, face count
};

struct GpuFace {
    header: vec4<u32>,    // kind, flip, first trim loop, trim loop count
    triangles: vec4<u32>, // first triangle vertex, triangle count
    params0: vec4<f32>,   // origin or apex, radius or half-angle tangent
    params1: vec4<f32>,   // plane normal or revolution axis, u period
    params2: vec4<f32>,   // plane u axis or revolution reference direction
    params3: vec4<f32>,   // plane v axis
    spline0: vec4<u32>,   // degree u, degree v, knot u start, knot v start
    spline1: vec4<u32>,   // knot u count, knot v count, control start, patch start
    spline2: vec4<u32>,   // patch count
};

struct GpuPatch {
    uv: vec4<f32>,      // u0, u1, v0, v1
    minimum: vec4<f32>,
    maximum: vec4<f32>,
};

struct GpuLoop {
    range: vec2<u32>, // first trim point, point count
};

struct Camera {
    origin: vec4<f32>,
    forward: vec4<f32>, // xyz, w = tan(half vertical fov)
    right: vec4<f32>,   // xyz, w = aspect ratio
    up: vec4<f32>,
    light: vec4<f32>, // xyz = direction towards the light
    size: vec4<u32>,  // width, height
};

@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read> nodes: array<GpuNode>;
@group(0) @binding(2) var<storage, read> faces: array<GpuFace>;
@group(0) @binding(3) var<storage, read> trim_loops: array<GpuLoop>;
@group(0) @binding(4) var<storage, read> trim_points: array<vec2<f32>>;
@group(0) @binding(5) var<storage, read> vertices: array<vec4<f32>>;

@group(0) @binding(6) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(7) var<storage, read> knots: array<f32>;
@group(0) @binding(8) var<storage, read> control: array<vec4<f32>>;
@group(0) @binding(9) var<storage, read> patches: array<GpuPatch>;

struct Candidate {
    distance: f32,
    uv: vec2<f32>,
    normal: vec3<f32>,
    valid: bool,
};

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) invocation: vec3<u32>) {
    let size = vec2<f32>(f32(camera.size.x), f32(camera.size.y));
    if (f32(invocation.x) >= size.x || f32(invocation.y) >= size.y) {
        return;
    }

    let uv = (vec2<f32>(f32(invocation.x), f32(invocation.y)) + vec2<f32>(0.5)) / size;
    let ndc = uv * 2.0 - vec2<f32>(1.0);
    let tan_half = camera.forward.w;
    let origin = camera.origin.xyz;
    let direction = normalize(
        camera.forward.xyz
            + camera.right.xyz * (ndc.x * tan_half * camera.right.w)
            - camera.up.xyz * (ndc.y * tan_half)
    );

    var best_distance = FAR;
    var best_normal = vec3<f32>(0.0);
    var hit = false;

    var stack: array<u32, STACK_DEPTH>;
    var stack_size = 0u;
    stack[0] = 0u;
    stack_size = 1u;

    loop {
        if (stack_size == 0u) {
            break;
        }
        stack_size -= 1u;
        let node = nodes[stack[stack_size]];
        if (slab_entry(node.minimum.xyz, node.maximum.xyz, origin, direction) >= best_distance) {
            continue;
        }
        let count = node.links.w;
        if (count == 0u) {
            if (stack_size + 2u <= STACK_DEPTH) {
                stack[stack_size] = node.links.x;
                stack_size += 1u;
                stack[stack_size] = node.links.y;
                stack_size += 1u;
            }
            continue;
        }
        for (var slot = 0u; slot < count; slot += 1u) {
            let index = node.links.z + slot;
            if (index >= arrayLength(&faces)) {
                continue;
            }
            let candidate = trace_face(faces[index], origin, direction, best_distance);
            if (candidate.valid) {
                best_distance = candidate.distance;
                best_normal = candidate.normal;
                hit = true;
            }
        }
    }

    var color = vec3<f32>(0.04, 0.05, 0.07);
    if (hit) {
        let normal = normalize(best_normal);
        let light = normalize(-camera.light.xyz);
        let diffuse = abs(dot(normal, light));
        color = vec3<f32>(0.58, 0.68, 0.82) * (0.22 + 0.78 * diffuse);
    }
    textureStore(output, vec2<i32>(invocation.xy), vec4<f32>(color, 1.0));
}

/// Nearest trimmed hit on `face`, closer than `limit`.
fn trace_face(face: GpuFace, origin: vec3<f32>, direction: vec3<f32>, limit: f32) -> Candidate {
    let kind = face.header.x;
    if (kind == KIND_TRIANGLES) {
        return trace_triangles(face, origin, direction, limit);
    }
    if (kind == KIND_PLANE) {
        return trace_plane(face, origin, direction, limit);
    }
    if (kind == KIND_SPHERE) {
        return trace_sphere(face, origin, direction, limit);
    }
    if (kind == KIND_SPLINE) {
        return trace_spline(face, origin, direction, limit);
    }
    return trace_revolved(face, origin, direction, limit, kind == KIND_CONE);
}

fn trace_sphere(
    face: GpuFace,
    origin: vec3<f32>,
    direction: vec3<f32>,
    limit: f32,
) -> Candidate {
    var result: Candidate;
    result.valid = false;
    result.distance = FAR;

    let centre = face.params0.xyz;
    let radius = face.params0.w;
    let axis = normalize(face.params1.xyz);
    let reference = face.params2.xyz;
    let rest = origin - centre;
    let a = dot(direction, direction);
    let b = 2.0 * dot(rest, direction);
    let c = dot(rest, rest) - radius * radius;
    let roots = quadratic_roots(a, b, c);
    for (var index = 0u; index < 2u; index += 1u) {
        let distance = roots[index];
        if (distance <= 1.0e-6 || distance >= result.distance || distance >= limit) {
            continue;
        }
        let position = origin + direction * distance;
        let normal = (position - centre) / radius;
        // Same parameterisation as the CPU caster, so trim loops placed by
        // either path land in the same space.
        let u = revolved_u(axis, reference, normal);
        let v = acos(clamp(dot(normal, axis), -1.0, 1.0));
        let uv = vec2<f32>(u, v);
        if (!inside_trims(face, uv)) {
            continue;
        }
        result.distance = distance;
        result.uv = uv;
        result.normal = orient(face, normal);
        result.valid = true;
    }
    return result;
}

fn trace_plane(face: GpuFace, origin: vec3<f32>, direction: vec3<f32>, limit: f32) -> Candidate {
    var result: Candidate;
    result.valid = false;
    result.distance = FAR;

    let normal = face.params1.xyz;
    let denominator = dot(normal, direction);
    if (abs(denominator) < 1.0e-12) {
        return result;
    }
    let anchor = face.params0.xyz;
    let distance = dot(anchor - origin, normal) / denominator;
    if (distance <= 1.0e-6 || distance >= limit) {
        return result;
    }

    let position = origin + direction * distance;
    let local = position - anchor;
    let uv = vec2<f32>(dot(local, face.params2.xyz), dot(local, face.params3.xyz));
    if (!inside_trims(face, uv)) {
        return result;
    }

    result.distance = distance;
    result.uv = uv;
    result.normal = orient(face, normalize(normal));
    result.valid = true;
    return result;
}

fn trace_revolved(
    face: GpuFace,
    origin: vec3<f32>,
    direction: vec3<f32>,
    limit: f32,
    cone: bool,
) -> Candidate {
    var result: Candidate;
    result.valid = false;
    result.distance = FAR;

    let axis = normalize(face.params1.xyz);
    let reference = face.params2.xyz;
    let anchor = face.params0.xyz;
    // `a`, `b`, `c` of the implicit quadratic: planes through the axis for a
    // cylinder, an apex quadric for a cone.
    var a: f32;
    var b: f32;
    var c: f32;
    var open: f32 = 0.0;
    if (cone) {
        let tangent = face.params0.w;
        open = 1.0 + tangent * tangent;
        let rest = origin - anchor;
        let axial_origin = dot(rest, axis);
        let axial_direction = dot(direction, axis);
        a = 1.0 - open * axial_direction * axial_direction;
        b = 2.0 * (dot(rest, direction) - open * axial_origin * axial_direction);
        c = dot(rest, rest) - open * axial_origin * axial_origin;
    } else {
        let radius = face.params0.w;
        let rest = origin - anchor;
        let radial_origin = rest - axis * dot(rest, axis);
        let radial_direction = direction - axis * dot(direction, axis);
        a = dot(radial_direction, radial_direction);
        b = 2.0 * dot(radial_origin, radial_direction);
        c = dot(radial_origin, radial_origin) - radius * radius;
    }

    let roots = quadratic_roots(a, b, c);
    for (var index = 0u; index < 2u; index += 1u) {
        let distance = roots[index];
        if (distance <= 1.0e-6 || distance >= result.distance || distance >= limit) {
            continue;
        }
        let position = origin + direction * distance;
        let local = position - anchor;
        let height = dot(local, axis);
        let radial = local - axis * height;
        var normal: vec3<f32>;
        if (cone) {
            let tangent = face.params0.w;
            normal = radial - axis * (tangent * tangent * height);
        } else {
            normal = radial;
        }
        if (length(normal) < 1.0e-9) {
            continue;
        }
        let uv = vec2<f32>(revolved_u(axis, reference, radial), height);
        if (!inside_trims(face, uv)) {
            continue;
        }
        result.distance = distance;
        result.uv = uv;
        result.normal = orient(face, normalize(normal));
        result.valid = true;
    }
    return result;
}

struct BasisWindow {
    base: i32,
    values: array<f32, 9>,
};

struct Frame {
    point: vec3<f32>,
    derivative_u: vec3<f32>,
    derivative_v: vec3<f32>,
    normal: vec3<f32>,
    valid: bool,
};

fn divide_or_zero(value: f32, width: f32) -> f32 {
    if (abs(width) > 1.0e-12) {
        return value / width;
    }
    return 0.0;
}

fn basis_value(window: BasisWindow, index: i32) -> f32 {
    let offset = index - window.base;
    if (offset < 0 || offset > i32(MAX_DEGREE)) {
        return 0.0;
    }
    return window.values[u32(offset)];
}

/// Nonzero B-spline basis functions in the span containing `t`, plus the index
/// of the first one. Mirrors `basis_plain` in the CPU caster.
fn basis_window(knot_start: u32, count: u32, degree: u32, t: f32) -> BasisWindow {
    var result: BasisWindow;
    result.base = 0;
    for (var i = 0u; i < 9u; i += 1u) {
        result.values[i] = 0.0;
    }
    if (degree > MAX_DEGREE || count <= degree) {
        return result;
    }
    let low = knots[knot_start + degree];
    let high = knots[knot_start + count];
    let clamped = clamp(t, low, high);
    var span = degree;
    while (span + 1u < count && clamped >= knots[knot_start + span + 1u]) {
        span += 1u;
    }
    result.base = i32(span) - i32(degree);
    if (degree == 0u) {
        result.values[0] = 1.0;
        return result;
    }

    var local: array<f32, 9>;
    var left: array<f32, 9>;
    var right: array<f32, 9>;
    for (var i = 0u; i < 9u; i += 1u) {
        local[i] = 0.0;
        left[i] = 0.0;
        right[i] = 0.0;
    }
    local[0] = 1.0;
    for (var j = 1u; j <= degree; j += 1u) {
        left[j] = clamped - knots[knot_start + span + 1u - j];
        right[j] = knots[knot_start + span + j] - clamped;
        var saved = 0.0;
        for (var r = 0u; r < j; r += 1u) {
            let denominator = right[r + 1u] + left[j - r];
            var temp = 0.0;
            if (abs(denominator) > 1.0e-12) {
                temp = local[r] / denominator;
            }
            local[r] = saved + right[r + 1u] * temp;
            saved = left[j - r] * temp;
        }
        local[j] = saved;
    }
    for (var i = 0u; i <= degree; i += 1u) {
        result.values[i] = local[i];
    }
    return result;
}

/// First derivatives of the nonzero basis functions.
fn basis_derivative_window(
    knot_start: u32,
    count: u32,
    degree: u32,
    t: f32,
) -> BasisWindow {
    var result: BasisWindow;
    result.base = 0;
    for (var i = 0u; i < 9u; i += 1u) {
        result.values[i] = 0.0;
    }
    if (degree == 0u || degree > MAX_DEGREE || count <= degree) {
        return result;
    }
    let lower = basis_window(knot_start, count + 1u, degree - 1u, t);
    let clamped = clamp(t, knots[knot_start + degree], knots[knot_start + count]);
    var span = degree;
    while (span + 1u < count && clamped >= knots[knot_start + span + 1u]) {
        span += 1u;
    }
    result.base = i32(span) - i32(degree);
    for (var r = 0u; r <= degree; r += 1u) {
        let index = i32(span) - i32(degree) + i32(r);
        let first = basis_value(lower, index);
        let second = basis_value(lower, index + 1);
        let width1 = knots[knot_start + u32(index) + degree] - knots[knot_start + u32(index)];
        let width2 = knots[knot_start + u32(index) + degree + 1u]
            - knots[knot_start + u32(index) + 1u];
        result.values[r] = f32(degree) * (divide_or_zero(first, width1) - divide_or_zero(second, width2));
    }
    return result;
}

fn perpendicular(direction: vec3<f32>) -> vec3<f32> {
    let candidate = select(
        vec3<f32>(1.0, 0.0, 0.0),
        vec3<f32>(0.0, 1.0, 0.0),
        abs(direction.x) >= 0.9,
    );
    return normalize(cross(direction, candidate));
}

fn spline_frame(face: GpuFace, u: f32, v: f32) -> Frame {
    var result: Frame;
    result.valid = false;
    let degree_u = face.spline0.x;
    let degree_v = face.spline0.y;
    if (degree_u > MAX_DEGREE || degree_v > MAX_DEGREE) {
        return result;
    }
    let knot_u_start = face.spline0.z;
    let knot_v_start = face.spline0.w;
    let knot_u_count = face.spline1.x;
    let knot_v_count = face.spline1.y;
    let control_start = face.spline1.z;
    let count_u = knot_u_count - degree_u - 1u;
    let count_v = knot_v_count - degree_v - 1u;
    if (count_u == 0u || count_v == 0u) {
        return result;
    }
    let u_values = basis_window(knot_u_start, count_u, degree_u, u);
    let u_derivatives = basis_derivative_window(knot_u_start, count_u, degree_u, u);
    let v_values = basis_window(knot_v_start, count_v, degree_v, v);
    let v_derivatives = basis_derivative_window(knot_v_start, count_v, degree_v, v);

    var point = vec4<f32>(0.0);
    var along_u = vec4<f32>(0.0);
    var along_v = vec4<f32>(0.0);
    for (var i = 0u; i <= degree_u; i += 1u) {
        for (var j = 0u; j <= degree_v; j += 1u) {
            let index = control_start
                + (u32(u_values.base) + i) * count_v
                + (u32(v_values.base) + j);
            if (index >= arrayLength(&control)) {
                continue;
            }
            let value = control[index];
            point += value * (u_values.values[i] * v_values.values[j]);
            along_u += value * (u_derivatives.values[i] * v_values.values[j]);
            along_v += value * (u_values.values[i] * v_derivatives.values[j]);
        }
    }

    let weight = point.w;
    if (abs(weight) <= 1.0e-12) {
        return result;
    }
    let position = point.xyz / weight;
    // Quotient rule for the rational surface.
    let derivative_u = (along_u.xyz - along_u.w * position) / weight;
    let derivative_v = (along_v.xyz - along_v.w * position) / weight;
    let normal = cross(derivative_u, derivative_v);
    if (length(normal) < 1.0e-12) {
        return result;
    }
    result.point = position;
    result.derivative_u = derivative_u;
    result.derivative_v = derivative_v;
    result.normal = normalize(normal);
    result.valid = true;
    return result;
}

fn trace_spline(
    face: GpuFace,
    origin: vec3<f32>,
    direction: vec3<f32>,
    limit: f32,
) -> Candidate {
    var result: Candidate;
    result.valid = false;
    result.distance = FAR;

    let degree_u = face.spline0.x;
    let degree_v = face.spline0.y;
    let knot_u_start = face.spline0.z;
    let knot_v_start = face.spline0.w;
    let count_u = face.spline1.x - degree_u - 1u;
    let count_v = face.spline1.y - degree_v - 1u;
    if (count_u == 0u || count_v == 0u) {
        return result;
    }
    let normal_axis = perpendicular(direction);
    let bitangent = cross(direction, normal_axis);
    let patch_start = face.spline1.w;
    let patch_count = face.spline2.x;

    for (var index = 0u; index < patch_count; index += 1u) {
        let region = patches[patch_start + index];
        let entry = slab_entry(region.minimum.xyz, region.maximum.xyz, origin, direction);
        if (entry >= limit) {
            continue;
        }
        var u = (region.uv.x + region.uv.y) * 0.5;
        var v = (region.uv.z + region.uv.w) * 0.5;
        var converged = false;
        for (var step = 0u; step < 16u; step += 1u) {
            let frame = spline_frame(face, u, v);
            if (!frame.valid) {
                break;
            }
            let gap = frame.point - origin;
            let f = dot(gap, normal_axis);
            let g = dot(gap, bitangent);
            if (abs(f) < 1.0e-6 && abs(g) < 1.0e-6) {
                converged = true;
                break;
            }
            let a = dot(frame.derivative_u, normal_axis);
            let b = dot(frame.derivative_v, normal_axis);
            let c = dot(frame.derivative_u, bitangent);
            let d = dot(frame.derivative_v, bitangent);
            let determinant = a * d - b * c;
            if (abs(determinant) < 1.0e-14) {
                break;
            }
            let du = (f * d - b * g) / determinant;
            let dv = (a * g - f * c) / determinant;
            u = clamp(u - du, region.uv.x, region.uv.y);
            v = clamp(v - dv, region.uv.z, region.uv.w);
        }
        let frame = spline_frame(face, u, v);
        if (!frame.valid) {
            continue;
        }
        if (!converged) {
            let gap = frame.point - origin;
            let residual = max(abs(dot(gap, normal_axis)), abs(dot(gap, bitangent)));
            // f32 cannot reach the f64 caster's 1e-6 residual on every ray.
            if (residual >= 1.0e-4) {
                continue;
            }
        }
        let distance = dot(frame.point - origin, direction);
        if (distance <= 1.0e-9
            || distance < entry - 1.0e-6
            || distance >= result.distance
            || distance >= limit) {
            continue;
        }
        let uv = vec2<f32>(u, v);
        if (!inside_trims(face, uv)) {
            continue;
        }
        result.distance = distance;
        result.uv = uv;
        result.normal = orient(face, frame.normal);
        result.valid = true;
    }
    return result;
}

fn trace_triangles(
    face: GpuFace,
    origin: vec3<f32>,
    direction: vec3<f32>,
    limit: f32,
) -> Candidate {
    var result: Candidate;
    result.valid = false;
    result.distance = FAR;

    let first = face.triangles.x;
    let count = face.triangles.y;
    for (var index = 0u; index < count; index += 1u) {
        let base = (first + index) * 3u;
        if (base + 2u >= arrayLength(&vertices)) {
            continue;
        }
        let a = vertices[base].xyz;
        let b = vertices[base + 1u].xyz;
        let c = vertices[base + 2u].xyz;
        let distance = triangle_hit(origin, direction, a, b, c);
        if (distance <= 1.0e-6 || distance >= result.distance || distance >= limit) {
            continue;
        }
        result.distance = distance;
        result.normal = orient(face, normalize(cross(b - a, c - a)));
        result.valid = true;
    }
    return result;
}

fn triangle_hit(
    origin: vec3<f32>,
    direction: vec3<f32>,
    a: vec3<f32>,
    b: vec3<f32>,
    c: vec3<f32>,
) -> f32 {
    let edge1 = b - a;
    let edge2 = c - a;
    let pvec = cross(direction, edge2);
    let determinant = dot(edge1, pvec);
    if (abs(determinant) < 1.0e-12) {
        return FAR;
    }
    let inverse = 1.0 / determinant;
    let tvec = origin - a;
    let u = dot(tvec, pvec) * inverse;
    if (u < 0.0 || u > 1.0) {
        return FAR;
    }
    let qvec = cross(tvec, edge1);
    let v = dot(direction, qvec) * inverse;
    if (v < 0.0 || u + v > 1.0) {
        return FAR;
    }
    return dot(edge2, qvec) * inverse;
}

fn orient(face: GpuFace, normal: vec3<f32>) -> vec3<f32> {
    if (face.header.y != 0u) {
        return -normal;
    }
    return normal;
}

fn revolved_u(axis: vec3<f32>, reference: vec3<f32>, radial: vec3<f32>) -> f32 {
    let r = normalize(reference - axis * dot(reference, axis));
    let tangent = cross(axis, r);
    let angle = atan2(dot(radial, tangent), dot(radial, r));
    if (angle < 0.0) {
        return angle + TAU;
    }
    return angle;
}

fn inside_trims(face: GpuFace, uv: vec2<f32>) -> bool {
    let first = face.header.z;
    let count = face.header.w;
    if (count == 0u) {
        return true;
    }
    let period = face.params1.w;
    var inside = false;
    for (var index = 0u; index < count; index += 1u) {
        let range = trim_loops[first + index].range;
        if (range.y < 3u) {
            continue;
        }
        var u = uv.x;
        if (period > 0.0) {
            u = unwrap_near(u, range, period);
        }
        if (point_in_loop(range, vec2<f32>(u, uv.y))) {
            inside = !inside;
        }
    }
    return inside;
}

fn point_in_loop(range: vec2<u32>, point: vec2<f32>) -> bool {
    var inside = false;
    var previous = trim_points[range.x + range.y - 1u];
    for (var index = 0u; index < range.y; index += 1u) {
        let current = trim_points[range.x + index];
        if ((current.y > point.y) != (previous.y > point.y)) {
            let t = (point.y - current.y) / (previous.y - current.y);
            let crossing = current.x + t * (previous.x - current.x);
            if (point.x < crossing) {
                inside = !inside;
            }
        }
        previous = current;
    }
    return inside;
}

/// Mirrors the CPU caster: test `u` at the periodic image nearest the loop.
fn unwrap_near(u: f32, range: vec2<u32>, period: f32) -> f32 {
    var low = FAR;
    var high = -FAR;
    for (var index = 0u; index < range.y; index += 1u) {
        let value = trim_points[range.x + index].x;
        low = min(low, value);
        high = max(high, value);
    }
    let centre = (low + high) * 0.5;
    var best = u;
    var best_distance = abs(u - centre);
    for (var shift = -2; shift <= 2; shift += 1) {
        let candidate = u + f32(shift) * period;
        let distance = abs(candidate - centre);
        if (distance < best_distance) {
            best = candidate;
            best_distance = distance;
        }
    }
    return best;
}

fn quadratic_roots(a: f32, b: f32, c: f32) -> vec2<f32> {
    if (abs(a) < 1.0e-12) {
        if (abs(b) < 1.0e-12) {
            return vec2<f32>(FAR, FAR);
        }
        return vec2<f32>(-c / b, FAR);
    }
    let discriminant = b * b - 4.0 * a * c;
    if (discriminant < 0.0) {
        return vec2<f32>(FAR, FAR);
    }
    let root = sqrt(discriminant);
    return vec2<f32>((-b - root) / (2.0 * a), (-b + root) / (2.0 * a));
}

/// Entry distance of the ray into the box, or `FAR` when it misses.
fn slab_entry(
    minimum: vec3<f32>,
    maximum: vec3<f32>,
    origin: vec3<f32>,
    direction: vec3<f32>,
) -> f32 {
    var enter = -FAR;
    var exit = FAR;
    for (var axis = 0; axis < 3; axis += 1) {
        let o = origin[axis];
        let d = direction[axis];
        if (abs(d) < 1.0e-12) {
            if (o < minimum[axis] || o > maximum[axis]) {
                return FAR;
            }
            continue;
        }
        let inverse = 1.0 / d;
        var near = (minimum[axis] - o) * inverse;
        var far = (maximum[axis] - o) * inverse;
        if (near > far) {
            let swap = near;
            near = far;
            far = swap;
        }
        enter = max(enter, near);
        exit = min(exit, far);
        if (enter > exit) {
            return FAR;
        }
    }
    if (exit < 0.0) {
        return FAR;
    }
    return max(enter, 0.0);
}
