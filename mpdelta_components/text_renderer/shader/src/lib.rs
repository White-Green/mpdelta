#![cfg_attr(target_arch = "spirv", no_std, feature(asm_experimental_arch))]
// HACK(eddyb) can't easily see warnings otherwise from `spirv-builder` builds.
#![deny(warnings)]

use bytemuck_derive::{Pod, Zeroable};
use spirv_std::glam::Mat4;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
#[cfg_attr(feature = "vulkano", derive(vulkano::pipeline::graphics::vertex_input::Vertex))]
pub struct FontVertex {
    #[cfg_attr(feature = "vulkano", format(R32_SFLOAT))]
    pub x: f32,
    #[cfg_attr(feature = "vulkano", format(R32_SFLOAT))]
    pub y: f32,
    #[cfg_attr(feature = "vulkano", format(R32_UINT))]
    pub glyph: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct GlyphStyle {
    pub scale: f32,
    pub offset_x: f32,
    pub offset_y: f32,
    pub color: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Constant {
    pub transform: Mat4,
}

#[cfg(feature = "shader")]
pub mod shader {
    use crate::{Constant, FontVertex, GlyphStyle};
    use spirv_std::glam::{vec4, Vec4};
    use spirv_std::spirv;

    #[spirv(fragment)]
    pub fn main_fs(color: Vec4, output: &mut Vec4) {
        *output = color;
    }

    #[spirv(vertex)]
    pub fn main_vs(x: f32, y: f32, glyph: u32, #[spirv(storage_buffer, descriptor_set = 1, binding = 0)] glyph_data: &[GlyphStyle], #[spirv(push_constant)] constant: &Constant, #[spirv(position, invariant)] out_pos: &mut Vec4, out_color: &mut Vec4) {
        struct VSOut {
            position: Vec4,
            color: Vec4,
        }
        #[inline(always)]
        fn inner(vertex: FontVertex, glyph_data: &[GlyphStyle], constant: &Constant) -> VSOut {
            let glyph_data = glyph_data[vertex.glyph as usize];
            let color = Vec4::new((glyph_data.color >> 24 & 0xFF) as f32 / 255., (glyph_data.color >> 16 & 0xFF) as f32 / 255., (glyph_data.color >> 8 & 0xFF) as f32 / 255., (glyph_data.color & 0xFF) as f32 / 255.);
            VSOut {
                position: constant.transform * vec4(vertex.x * glyph_data.scale + glyph_data.offset_x, -vertex.y * glyph_data.scale + glyph_data.offset_y, 0., 1.),
                color,
            }
        }
        VSOut { position: *out_pos, color: *out_color } = inner(FontVertex { x, y, glyph }, glyph_data, constant);
    }
}
