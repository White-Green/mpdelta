#![cfg_attr(target_arch = "spirv", no_std, feature(register_attr), register_attr(spirv), feature(asm_experimental_arch))]
// HACK(eddyb) can't easily see warnings otherwise from `spirv-builder` builds.
// #![deny(warnings)]

use spirv_std::glam::Mat4;

pub struct TextureDrawingConstant {
    pub transform_matrix: Mat4,
}

#[cfg(feature = "shader")]
pub mod shader {
    use core::hint::unreachable_unchecked;
    #[cfg(not(target_arch = "spirv"))]
    use spirv_std::macros::spirv;

    use crate::TextureDrawingConstant;
    use spirv_std::glam::{vec4, Vec2, Vec4};
    use spirv_std::{Image, Sampler};

    #[spirv(fragment)]
    pub fn main_fs(#[spirv(descriptor_set = 0, binding = 0)] image: &Image!(2D, format=rgba8, sampled=true), #[spirv(descriptor_set = 0, binding = 1)] sampler: &Sampler, uv: Vec2, output: &mut Vec4, #[spirv(flat)] output_stencil: &mut u32) {
        *output = image.sample(*sampler, uv);
        // *output = Vec4::new(1., 0., 0., 1.);
        *output_stencil = 1;
    }

    #[spirv(vertex)]
    pub fn main_vs(#[spirv(vertex_index)] vert_id: u32, #[spirv(push_constant)] constant: &TextureDrawingConstant, #[spirv(position, invariant)] out_pos: &mut Vec4, #[spirv(invariant)] out_uv: &mut Vec2) {
        struct VSOut {
            position: Vec4,
            uv: Vec2,
        }
        #[inline(always)]
        fn inner(vert_id: u32, constant: &TextureDrawingConstant) -> VSOut {
            let (x, u) = match vert_id & 1 {
                0 => (1., 0.),
                1 => (-1., 1.),
                _ => unsafe { unreachable_unchecked() },
            };
            let (y, v) = match (vert_id >> 1) & 1 {
                0 => (-1., 1.),
                1 => (1., 0.),
                _ => unsafe { unreachable_unchecked() },
            };
            VSOut {
                position: constant.transform_matrix * vec4(x, y, 0., 1.),
                uv: Vec2::new(u, v),
            }
        }
        VSOut { position: *out_pos, uv: *out_uv } = inner(vert_id, constant);
    }
}
