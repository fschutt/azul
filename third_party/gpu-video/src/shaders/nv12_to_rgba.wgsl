struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) tex_coords: vec2<f32>,
}

const VERTICES: array<vec3<f32>, 3> = array<vec3<f32>, 3>(
    vec3<f32>(-3.0, 1.0, 0.0),
    vec3<f32>(1.0, -3.0, 0.0),
    vec3<f32>(1.0, 1.0, 0.0),
);

const TEXTURE_COORDS: array<vec2<f32>, 3> = array<vec2<f32>, 3>(
    vec2<f32>(-1.0, 0.0),
    vec2<f32>(1.0, 2.0),
    vec2<f32>(1.0, 0.0),
);

@vertex
fn vs_main(@builtin(vertex_index) idx: u32) -> VertexOutput {
    var output: VertexOutput;

    output.position = vec4(VERTICES[idx], 1.0);
    output.tex_coords = TEXTURE_COORDS[idx];

    return output;
}

@group(0) @binding(0) var y_texture: texture_2d<f32>;
@group(0) @binding(1) var uv_texture: texture_2d<f32>;
@group(1) @binding(0) var sampler_: sampler;

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // BT709 limited range
    let conversion_bias = vec3<f32>(16.0 / 255.0, 0.5, 0.5);
    let conversion_scale = vec3<f32>(255.0 / 219.0, 255.0 / 224.0, 255.0 / 224.0);
    let conversion_weights = mat3x3<f32>(
        1.0, 0.0, 1.5748,
        1.0, -0.1873, -0.4681,
        1.0, 1.8556, 0.0
    );

    let yuv = vec3<f32>(
        textureSample(y_texture, sampler_, input.tex_coords).r,
        textureSample(uv_texture, sampler_, input.tex_coords).rg,
    );
    let rgb = (yuv - conversion_bias) * conversion_scale * conversion_weights;
    return vec4<f32>(rgb, 1.0);
}
