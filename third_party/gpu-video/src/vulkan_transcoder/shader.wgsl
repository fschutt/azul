@group(0) @binding(0) var source_y: texture_storage_2d<r8unorm, read>;
@group(0) @binding(1) var source_uv: texture_storage_2d<rg8unorm, read>;

@group(1) @binding(0) var dest_y: binding_array<texture_storage_2d<r8unorm, write>, 8>;
@group(2) @binding(0) var dest_uv: binding_array<texture_storage_2d<rg8unorm, write>, 8>;

struct Immediates {
  output_number: u32,
  input_width: u32,
  input_height: u32,
  scaling: array<u32, 8>,
}

var<immediate> imm: Immediates;

const PI: f32 = 3.14159265358979323846;

fn sinc(x: f32) -> f32 {
  if abs(x) < 1e-6 {
    return 1.0;
  }
  let px = PI * x;
  return sin(px) / px;
}

fn lanczos3_weight(x: f32) -> f32 {
  if abs(x) >= 3.0 {
    return 0.0;
  }
  return sinc(x) * sinc(x / 3.0);
}

fn sample_nearest_y(float_coords: vec2<f32>, input_size: vec2<u32>) -> vec4<f32> {
  let coords = vec2<u32>(vec2<f32>(input_size) * float_coords);
  return textureLoad(source_y, coords);
}

fn sample_bilinear_y(float_coords: vec2<f32>, input_size: vec2<u32>) -> vec4<f32> {
  let fc = vec2<f32>(input_size) * float_coords - 0.5;
  let x0 = u32(max(floor(fc.x), 0.0));
  let y0 = u32(max(floor(fc.y), 0.0));
  let x1 = min(x0 + 1, input_size.x - 1);
  let y1 = min(y0 + 1, input_size.y - 1);
  let fx = fc.x - floor(fc.x);
  let fy = fc.y - floor(fc.y);

  let p00 = textureLoad(source_y, vec2(x0, y0)).r;
  let p10 = textureLoad(source_y, vec2(x1, y0)).r;
  let p01 = textureLoad(source_y, vec2(x0, y1)).r;
  let p11 = textureLoad(source_y, vec2(x1, y1)).r;

  let val = mix(mix(p00, p10, fx), mix(p01, p11, fx), fy);
  return vec4(val, 0.0, 0.0, 1.0);
}

fn sample_lanczos3_y(float_coords: vec2<f32>, input_size: vec2<u32>) -> vec4<f32> {
  let fc = vec2<f32>(input_size) * float_coords - 0.5;
  let center_x = floor(fc.x);
  let center_y = floor(fc.y);
  let max_x = i32(input_size.x) - 1;
  let max_y = i32(input_size.y) - 1;

  var sum = 0.0;
  var weight_sum = 0.0;
  for (var dy = -2; dy <= 3; dy++) {
    let sy = clamp(i32(center_y) + dy, 0, max_y);
    let wy = lanczos3_weight(fc.y - (center_y + f32(dy)));
    for (var dx = -2; dx <= 3; dx++) {
      let sx = clamp(i32(center_x) + dx, 0, max_x);
      let wx = lanczos3_weight(fc.x - (center_x + f32(dx)));
      let w = wx * wy;
      sum += textureLoad(source_y, vec2(u32(sx), u32(sy))).r * w;
      weight_sum += w;
    }
  }

  let val = sum / weight_sum;
  return vec4(val, 0.0, 0.0, 1.0);
}

fn sample_nearest_uv(float_coords: vec2<f32>, input_uv_size: vec2<u32>) -> vec4<f32> {
  let coords = vec2<u32>(vec2<f32>(input_uv_size) * float_coords);
  return textureLoad(source_uv, coords);
}

fn sample_bilinear_uv(float_coords: vec2<f32>, input_uv_size: vec2<u32>) -> vec4<f32> {
  let fc = vec2<f32>(input_uv_size) * float_coords - 0.5;
  let x0 = u32(max(floor(fc.x), 0.0));
  let y0 = u32(max(floor(fc.y), 0.0));
  let x1 = min(x0 + 1, input_uv_size.x - 1);
  let y1 = min(y0 + 1, input_uv_size.y - 1);
  let fx = fc.x - floor(fc.x);
  let fy = fc.y - floor(fc.y);

  let p00 = textureLoad(source_uv, vec2(x0, y0));
  let p10 = textureLoad(source_uv, vec2(x1, y0));
  let p01 = textureLoad(source_uv, vec2(x0, y1));
  let p11 = textureLoad(source_uv, vec2(x1, y1));

  let val = mix(mix(p00, p10, vec4(fx)), mix(p01, p11, vec4(fx)), vec4(fy));
  return val;
}

fn sample_lanczos3_uv(float_coords: vec2<f32>, input_uv_size: vec2<u32>) -> vec4<f32> {
  let fc = vec2<f32>(input_uv_size) * float_coords - 0.5;
  let center_x = floor(fc.x);
  let center_y = floor(fc.y);
  let max_x = i32(input_uv_size.x) - 1;
  let max_y = i32(input_uv_size.y) - 1;

  var sum = vec4(0.0);
  var weight_sum = 0.0;
  for (var dy = -2; dy <= 3; dy++) {
    let sy = clamp(i32(center_y) + dy, 0, max_y);
    let wy = lanczos3_weight(fc.y - (center_y + f32(dy)));
    for (var dx = -2; dx <= 3; dx++) {
      let sx = clamp(i32(center_x) + dx, 0, max_x);
      let wx = lanczos3_weight(fc.x - (center_x + f32(dx)));
      let w = wx * wy;
      sum += textureLoad(source_uv, vec2(u32(sx), u32(sy))) * w;
      weight_sum += w;
    }
  }

  return sum / weight_sum;
}

@compute
@workgroup_size(256)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
  var remaining_offset: u32 = id.x;
  var i: u32 = 0;
  for ( ; i < imm.output_number; i++ ) {
    let size = textureDimensions(dest_y[i]);
    let total_size = (size.x * size.y + 255) / 256 * 256;
    if (remaining_offset < total_size) {
      break;
    }

    remaining_offset -= total_size;
  }

  if i >= imm.output_number {
    return;
  }

  let size = textureDimensions(dest_y[i]);
  let y = remaining_offset / size.x;
  let x = remaining_offset % size.x;
  let coords_output = vec2(x, y);

  if y >= size.y {
    return;
  }

  let float_coords = (vec2<f32>(coords_output) + 0.5) / vec2<f32>(size);
  let input_size = vec2(imm.input_width, imm.input_height);
  let algo = imm.scaling[i];

  var output_y: vec4<f32>;
  if algo == 2u {
    output_y = sample_lanczos3_y(float_coords, input_size);
  } else if algo == 1u {
    output_y = sample_bilinear_y(float_coords, input_size);
  } else {
    output_y = sample_nearest_y(float_coords, input_size);
  }
  textureStore(dest_y[i], coords_output, output_y);

  if (x % 2 == 0 && y % 2 == 0) {
    let input_uv_size = input_size / 2;
    let uv_coords_output = coords_output / 2;
    let uv_size = size / 2;
    let float_coords_uv = (vec2<f32>(uv_coords_output) + 0.5) / vec2<f32>(uv_size);
    var output_uv: vec4<f32>;
    if algo == 2u {
      output_uv = sample_lanczos3_uv(float_coords_uv, input_uv_size);
    } else if algo == 1u {
      output_uv = sample_bilinear_uv(float_coords_uv, input_uv_size);
    } else {
      output_uv = sample_nearest_uv(float_coords_uv, input_uv_size);
    }
    textureStore(dest_uv[i], uv_coords_output, output_uv);
  }
}
