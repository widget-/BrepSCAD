struct Uniforms {
    view_proj: mat4x4<f32>,
    camera_pos: vec4<f32>,
    light_dir: vec4<f32>,
};

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

struct VertexOut {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) world_position: vec3<f32>,
};

@vertex
fn vs_main(
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
) -> VertexOut {
    var out: VertexOut;
    out.clip_position = uniforms.view_proj * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.world_position = position;
    return out;
}

@fragment
fn fs_main(in: VertexOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let l = normalize(-uniforms.light_dir.xyz);
    let diffuse = abs(dot(n, l));
    // Same convention as the analytic preview: a face seen from behind has its
    // normal pointing away from the eye. Paint those red so a missing front face
    // is visible instead of reading as an ordinary wall.
    let towards_eye = normalize(uniforms.camera_pos.xyz - in.world_position);
    let backside = dot(n, towards_eye) < 0.0;
    let base = select(
        vec3<f32>(0.58, 0.68, 0.82),
        vec3<f32>(0.86, 0.16, 0.14),
        backside,
    );
    let color = base * (0.22 + 0.78 * diffuse);
    return vec4<f32>(color, 1.0);
}
