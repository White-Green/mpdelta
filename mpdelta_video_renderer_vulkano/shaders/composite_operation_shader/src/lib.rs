#![cfg_attr(target_arch = "spirv", no_std, feature(register_attr), register_attr(spirv), feature(asm_experimental_arch))]
// HACK(eddyb) can't easily see warnings otherwise from `spirv-builder` builds.
#![deny(warnings)]

pub const BLOCK_SIZE: u32 = 32;

pub struct CompositeOperationConstant {
    pub composite: u32,
    pub blend: u32,
    pub image_width: u32,
    pub image_height: u32,
}

#[cfg(feature = "shader")]
pub mod shader {
    use crate::CompositeOperationConstant;
    use spirv_std::glam::{UVec2, Vec4, Vec4Swizzles};
    #[cfg(not(target_arch = "spirv"))]
    use spirv_std::macros::spirv;
    use spirv_std::Image;

    #[spirv(compute(threads(32, 32)))]
    pub fn main(
        #[spirv(global_invocation_id)] id: UVec2,
        #[spirv(descriptor_set = 0, binding = 0)] result_image: &Image!(2D, format=rgba8),
        #[spirv(descriptor_set = 1, binding = 0)] image: &Image!(2D, format=rgba8),
        #[spirv(descriptor_set = 1, binding = 0)] stencil: &Image!(2D, format=r32ui),
        #[spirv(push_constant)] constant: &CompositeOperationConstant,
    ) {
        if constant.image_width <= id.x || constant.image_height <= id.y {
            return;
        }
        let stencil: UVec2 = stencil.read(id);
        if stencil.x == 0 {
            return;
        }
        let dest_color: Vec4 = result_image.read(id);
        let src_color: Vec4 = image.read(id);
        let c_s = src_color.xyz();
        let a_s = src_color.w;
        let c_b = dest_color.xyz();
        let a_b = dest_color.w;
        // see mpdelta_core::component::parameter::BlendMode
        let co = match constant.blend {
            1/* Multiply */ => c_s,
            2/* Screen */ => c_s,
            3/* Overlay */ => c_s,
            4/* Darken */ => c_s,
            5/* Lighten */ => c_s,
            6/* ColorDodge */ => c_s,
            7/* ColorBurn */ => c_s,
            8/* HardLight */ => c_s,
            9/* SoftLight */ => c_s,
            10/* Difference */ => c_s,
            11/* Exclusion */ => c_s,
            12/* Hue */ => c_s,
            13/* Saturation */ => c_s,
            14/* Color */ => c_s,
            15/* Luminosity */ => c_s,
            _/* Normal */ => c_s,
        };
        let c_s = (1. - a_b) * c_s + a_b * co;
        // see mpdelta_core::component::parameter::CompositeOperation
        let (fa, fb) = match constant.composite {
            0/* Clear */ => (1., 1. - a_s),
            1/* Copy */ => (1., 1. - a_s),
            2/* Destination */ => (1., 1. - a_s),
            4/* DestinationOver */ => (1., 1. - a_s),
            5/* SourceIn */ => (1., 1. - a_s),
            6/* DestinationIn */ => (1., 1. - a_s),
            7/* SourceOut */ => (1., 1. - a_s),
            8/* DestinationOut */ => (1., 1. - a_s),
            9/* SourceAtop */ => (1., 1. - a_s),
            10/* DestinationAtop */ => (1., 1. - a_s),
            11/* XOR */ => (1., 1. - a_s),
            12/* Lighter */ => (1., 1. - a_s),
            _/* SourceOver */ => (1., 1. - a_s),
        };
        let co = a_s * fa * c_s + a_b * fb * c_b;
        let ao = (a_s * fa + a_b * fb).clamp(0.0, 1.0);
        let result_color = if ao == 0.0 { Vec4::new(0., 0., 0., 0.) } else { (co / ao).extend(ao) };
        unsafe { result_image.write(id, result_color) };
    }
}
