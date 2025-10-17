// src/common.rs

use crate::common_types::*;
use glam::{vec2, vec4, ivec2, ivec4, mat4, Mat4, Vec2, Vec4};

// This context will be passed to every shader function. It holds references
// to the resources (samplers) that the shaders need to access.
pub struct ShaderContext<'a> {
    pub s_clip_mask: &'a dyn Sampler2D,
    pub s_color0: &'a dyn Sampler2D,
    pub s_color1: &'a dyn Sampler2D,
    pub s_color2: &'a dyn Sampler2D,
    pub s_gpu_cache: &'a dyn Sampler2D,
    pub s_primitive_headers_f: &'a dyn Sampler2D,
    pub s_primitive_headers_i: &'a dyn ISampler2D,
    pub s_render_tasks: &'a dyn Sampler2D,
    pub s_transform_palette: &'a dyn Sampler2D,
}

//Enum to to identify samplers
pub enum SamplerId {
    SColor0,
    SColor1,
    SColor2,
    // Add other samplers as needed
}

impl<'a> ShaderContext<'a> {
    //
    // 1. Core Data Fetching
    //
    
    pub fn decode_instance_attributes(&self, a_data: ivec4) -> Instance {
        Instance {
            prim_header_address: a_data.x,
            clip_address: a_data.y,
            segment_index: (a_data.z) & 0xFFFF,
            flags: (a_data.z) >> 16,
            resource_address: (a_data.w) & 0xFFFFFF,
            brush_kind: (a_data.w) >> 24,
        }
    }

    pub fn fetch_prim_header(&self, index: i32) -> PrimitiveHeader {
        let uv_f = ivec2::new((2 * index) % 1024, (2 * index) / 1024);
        let local_rect_vec = self.s_primitive_headers_f.texel_fetch(uv_f, 0);
        let local_clip_rect_vec = self.s_primitive_headers_f.texel_fetch(uv_f + ivec2::new(1, 0), 0);

        let uv_i = uv_f; // Same UV for the integer texture
        let data0 = self.s_primitive_headers_i.texel_fetch(uv_i, 0);
        let data1 = self.s_primitive_headers_i.texel_fetch(uv_i + ivec2::new(1, 0), 0);
        
        PrimitiveHeader {
            local_rect: RectWithEndpoint { p0: local_rect_vec.xy(), p1: local_rect_vec.zw() },
            local_clip_rect: RectWithEndpoint { p0: local_clip_rect_vec.xy(), p1: local_clip_rect_vec.zw() },
            z: f32::from_bits(data0.x as u32),
            specific_prim_address: data0.y,
            transform_id: data0.z,
            picture_task_address: data0.w,
            user_data: data1,
        }
    }
    
    pub fn fetch_transform(&self, id: i32) -> Transform {
        let is_axis_aligned = (id >> 23) == 0;
        let index = id & 0x7FFFFF;

        let uv = ivec2::new((8 * index) % 1024, (8 * index) / 1024);
        
        let m0 = self.s_transform_palette.texel_fetch(uv + ivec2::new(0, 0), 0);
        let m1 = self.s_transform_palette.texel_fetch(uv + ivec2::new(1, 0), 0);
        let m2 = self.s_transform_palette.texel_fetch(uv + ivec2::new(2, 0), 0);
        let m3 = self.s_transform_palette.texel_fetch(uv + ivec2::new(3, 0), 0);
        
        let inv_m0 = self.s_transform_palette.texel_fetch(uv + ivec2::new(4, 0), 0);
        let inv_m1 = self.s_transform_palette.texel_fetch(uv + ivec2::new(5, 0), 0);
        let inv_m2 = self.s_transform_palette.texel_fetch(uv + ivec2::new(6, 0), 0);
        let inv_m3 = self.s_transform_palette.texel_fetch(uv + ivec2::new(7, 0), 0);

        Transform {
            m: Mat4::from_cols(m0, m1, m2, m3),
            inv_m: Mat4::from_cols(inv_m0, inv_m1, inv_m2, inv_m3),
            is_axis_aligned,
        }
    }
    
    pub fn fetch_render_task_data(&self, index: i32) -> (RectWithEndpoint, vec4) {
        let uv = ivec2::new((2 * index) % 1024, (2 * index) / 1024);
        let texel0 = self.s_render_tasks.texel_fetch(uv, 0);
        let texel1 = self.s_render_tasks.texel_fetch(uv + ivec2::new(1, 0), 0);
        let task_rect = RectWithEndpoint { p0: texel0.xy(), p1: texel0.zw() };
        (task_rect, texel1)
    }

    pub fn fetch_picture_task(&self, address: i32) -> PictureTask {
        let (task_rect, user_data) = self.fetch_render_task_data(address);
        PictureTask {
            task_rect,
            device_pixel_scale: user_data.x,
            content_origin: user_data.yz(),
        }
    }

    pub fn fetch_clip_area(&self, index: i32) -> ClipArea {
        if index >= i32::MAX {
            return ClipArea::default();
        }
        let (task_rect, user_data) = self.fetch_render_task_data(index);
        ClipArea {
            task_rect,
            device_pixel_scale: user_data.x,
            screen_origin: user_data.yz(),
        }
    }

    //
    // 2. GPU Cache Accessors
    //
    
    fn get_gpu_cache_uv(&self, address: i32) -> ivec2 {
        ivec2::new(address % 1024, address / 1024)
    }

    pub fn fetch_from_gpu_cache_1(&self, address: i32) -> vec4 {
        let uv = self.get_gpu_cache_uv(address);
        self.s_gpu_cache.texel_fetch(uv, 0)
    }

    pub fn fetch_from_gpu_cache_2(&self, address: i32) -> [vec4; 2] {
        let uv = self.get_gpu_cache_uv(address);
        [
            self.s_gpu_cache.texel_fetch(uv, 0),
            self.s_gpu_cache.texel_fetch(uv + ivec2::new(1, 0), 0),
        ]
    }
    
    pub fn fetch_from_gpu_cache_3(&self, address: i32) -> [vec4; 3] {
        let uv = self.get_gpu_cache_uv(address);
        [
            self.s_gpu_cache.texel_fetch(uv, 0),
            self.s_gpu_cache.texel_fetch(uv + ivec2::new(1, 0), 0),
            self.s_gpu_cache.texel_fetch(uv + ivec2::new(2, 0), 0),
        ]
    }

    pub fn fetch_from_gpu_cache_4(&self, address: i32) -> [vec4; 4] {
        let uv = self.get_gpu_cache_uv(address);
        [
            self.s_gpu_cache.texel_fetch(uv, 0),
            self.s_gpu_cache.texel_fetch(uv + ivec2::new(1, 0), 0),
            self.s_gpu_cache.texel_fetch(uv + ivec2::new(2, 0), 0),
            self.s_gpu_cache.texel_fetch(uv + ivec2::new(3, 0), 0),
        ]
    }

    // Special GPU Cache accessors


    pub fn fetch_image_data(&self, address: i32) -> ImageBrushData {
        let raw_data = self.fetch_from_gpu_cache_3(address);
        ImageBrushData {
            color: raw_data[0],
            background_color: raw_data[1],
            stretch_size: raw_data[2].xy(),
        }
    }

    pub fn fetch_image_source(&self, address: i32) -> (RectWithEndpoint, vec4) {
        let data = self.fetch_from_gpu_cache_2(address);
        let uv_rect = RectWithEndpoint { p0: data[0].xy(), p1: data[0].zw() };
        (uv_rect, data[1])
    }
    
    pub fn fetch_image_source_extra(&self, address: i32) -> ImageSourceExtra {
        let data = self.fetch_from_gpu_cache_4(address + 2); // Note the offset
        ImageSourceExtra {
            st_tl: data[0],
            st_tr: data[1],
            st_bl: data[2],
            st_br: data[3],
        }
    }

    //
    // 3. Vertex Processing & Output
    //
    
    pub fn write_vertex(&self, local_pos: vec2, local_clip_rect: RectWithEndpoint, z: f32, transform: &Transform, task: &PictureTask, u_transform: &mat4) -> (vec4, VertexInfo) {
        let clamped_local_pos = local_pos.clamp(local_clip_rect.p0, local_clip_rect.p1);
        let world_pos = transform.m * vec4::new(clamped_local_pos.x, clamped_local_pos.y, 0.0, 1.0);
        let device_pos = world_pos.xy() * task.device_pixel_scale;
        let final_offset = -task.content_origin + task.task_rect.p0;
        
        let gl_position = *u_transform * vec4::new(
            device_pos.x + (final_offset.x * world_pos.w),
            device_pos.y + (final_offset.y * world_pos.w),
            z * world_pos.w,
            world_pos.w,
        );

        let vi = VertexInfo { local_pos: clamped_local_pos, world_pos };
        (gl_position, vi)
    }
    
    // `swgl_clipMask` and `swgl_antiAlias` would be methods on the rasterizer state,
    // so we assume they are available in the scope where this is called.
    
    //
    // 4. Geometric and Math Utilities
    //
    
    pub fn rect_size(&self, rect: RectWithEndpoint) -> vec2 {
        rect.p1 - rect.p0
    }
    
    /// Corresponds to `get_image_quad_uv`
    pub fn get_image_quad_uv(&self, address: i32, f: Vec2) -> Vec2 {
        let extra_data = self.fetch_image_source_extra(address);
        let x = extra_data.st_tl.lerp(extra_data.st_tr, f.x);
        let y = extra_data.st_bl.lerp(extra_data.st_br, f.x);
        let z = x.lerp(y, f.y);
        z.xy() / z.w
    }

    //
    // 5. Texture & Sampler Helpers
    //

    /// Gets the size of a texture for a given sampler.
    /// In a real implementation, this would query the backend.
    pub fn texture_size(&self, sampler: SamplerId, _lod: i32) -> Vec2 {
        // Placeholder implementation. A real backend would provide this.
        match sampler {
            SamplerId::SColor0 => vec2(1024.0, 1024.0),
            SamplerId::SColor1 => vec2(1024.0, 1024.0),
            SamplerId::SColor2 => vec2(1024.0, 1024.0),
        }
    }
    
    /// Samples a texture. This is a wrapper to abstract the backend call.
    pub fn texture(&self, sampler: SamplerId, uv: Vec2) -> Vec4 {
        // Placeholder implementation.
        match sampler {
            SamplerId::SColor0 => self.s_color0.texture(uv),
            SamplerId::SColor1 => self.s_color1.texture(uv),
            SamplerId::SColor2 => self.s_color2.texture(uv),
        }
    }

}