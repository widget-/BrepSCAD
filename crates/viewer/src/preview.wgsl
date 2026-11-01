// Compute preview: one ray per pixel against the packed B-rep scene.
//
// This mirrors `brepscad_raycast` (planes, cylinders, cones, triangle-covered
// faces, even-odd trim loops, median-split BVH) in single precision. The CPU
// caster stays the reference: if the two disagree, the CPU one is right.

const KIND_PLANE: u32 = 0u;
const KIND_CYLINDER: u32 = 1u;
const KIND_CONE: u32 = 2u;
const KIND_TRIANGLES: u32 = 3u;

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
    return trace_revolved(face, origin, direction, limit, kind == KIND_CONE);
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
