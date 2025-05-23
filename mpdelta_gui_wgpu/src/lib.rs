use eframe::epaint::TextureId;
use eframe::Frame;
use egui::{Context, ViewportId};
use egui_wgpu::{WgpuSetup, WgpuSetupExisting};
use mpdelta_core_vulkano::ImageType;
use mpdelta_gui::view::Gui;
use mpdelta_gui::ImageRegister;
use std::time::Instant;
use vulkano::image::ImageUsage as VulkanoImageUsage;
use vulkano::VulkanObject;
use wgpu::hal::{MemoryFlags as WgpuHalMemoryFlags, TextureUses as WgpuHalTextureUses};
use wgpu::{Adapter as WgpuAdapter, Device as WgpuDevice, Extent3d, FilterMode, Instance as WgpuInstance, Queue as WgpuQueue, TextureUsages as WgpuTextureUsages};
use winit::window::WindowId;

#[derive(Debug)]
pub enum UserEvent {
    RequestRepaint { viewport_id: ViewportId, when: Instant, frame_nr: u64 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventResult {
    Wait,
    RepaintNow(WindowId),
    RepaintNext(WindowId),
    RepaintAt(WindowId, Instant),
    Exit,
}

pub struct MPDeltaGUIWgpu<T> {
    instance: WgpuInstance,
    adapter: WgpuAdapter,
    device: WgpuDevice,
    queue: WgpuQueue,
    gui: T,
}

impl<T: Gui<ImageType> + 'static> MPDeltaGUIWgpu<T> {
    pub fn new(instance: WgpuInstance, adapter: WgpuAdapter, device: WgpuDevice, queue: WgpuQueue, gui: T) -> MPDeltaGUIWgpu<T> {
        MPDeltaGUIWgpu { instance, adapter, device, queue, gui }
    }

    pub fn main(self) {
        let MPDeltaGUIWgpu { instance, adapter, device, queue, gui } = self;

        let native_options = eframe::NativeOptions {
            wgpu_options: egui_wgpu::WgpuConfiguration {
                wgpu_setup: WgpuSetup::from(WgpuSetupExisting { instance, adapter, device, queue }),
                ..egui_wgpu::WgpuConfiguration::default()
            },
            ..eframe::NativeOptions::default()
        };
        eframe::run_native("mpdelta", native_options, Box::new(move |ctx| Ok(Box::new(MPDeltaEframeApp::new(gui, ctx))))).unwrap()
    }
}

struct MPDeltaEframeApp<T> {
    gui: T,
}

impl<T> MPDeltaEframeApp<T>
where
    T: Gui<ImageType> + 'static,
{
    fn new(mut gui: T, ctx: &eframe::CreationContext) -> Self {
        gui.init(&ctx.egui_ctx);
        MPDeltaEframeApp { gui }
    }
}

struct WgpuImageRegister<'a> {
    device: &'a WgpuDevice,
    renderer: &'a mut egui_wgpu::Renderer,
}

impl<'a> WgpuImageRegister<'a> {
    fn new(device: &'a WgpuDevice, renderer: &'a mut egui_wgpu::Renderer) -> WgpuImageRegister<'a> {
        WgpuImageRegister { device, renderer }
    }
}

impl ImageRegister<ImageType> for WgpuImageRegister<'_> {
    fn register_image(&mut self, texture: ImageType) -> TextureId {
        let size = Extent3d {
            width: texture.0.extent()[0],
            height: texture.0.extent()[1],
            depth_or_array_layers: texture.0.extent()[2],
        };
        let dimension = wgpu::TextureDimension::D2;
        let sample_count = texture.0.samples().into();
        let mip_level_count = texture.0.mip_levels();
        let hal_descriptor = wgpu::hal::TextureDescriptor {
            label: None,
            size,
            mip_level_count,
            sample_count,
            dimension,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: vulkano_usage_into_wgpu_hal_usage(texture.0.usage()),
            memory_flags: vulkano_usage_into_wgpu_memory_flags(texture.0.usage()),
            view_formats: vec![wgpu::TextureFormat::Rgba8UnormSrgb],
        };
        let descriptor = wgpu::TextureDescriptor {
            label: None,
            size,
            mip_level_count,
            sample_count,
            dimension,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: vulkano_usage_into_wgpu_usage(texture.0.usage()),
            view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
        };
        let texture = unsafe {
            let texture = wgpu::hal::vulkan::Device::texture_from_raw(
                texture.0.handle(),
                &hal_descriptor,
                Some(Box::new(move || {
                    let _ = texture;
                })),
            );
            self.device.create_texture_from_hal::<wgpu::hal::vulkan::Api>(texture, &descriptor)
        };
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        self.renderer.register_native_texture(self.device, &texture_view, FilterMode::Linear)
    }

    fn unregister_image(&mut self, id: TextureId) {
        self.renderer.free_texture(&id)
    }
}

fn vulkano_usage_into_wgpu_usage(usage: VulkanoImageUsage) -> WgpuTextureUsages {
    [
        (VulkanoImageUsage::TRANSFER_SRC, WgpuTextureUsages::COPY_SRC),
        (VulkanoImageUsage::TRANSFER_DST, WgpuTextureUsages::COPY_DST),
        (VulkanoImageUsage::SAMPLED, WgpuTextureUsages::TEXTURE_BINDING),
        (VulkanoImageUsage::STORAGE, WgpuTextureUsages::STORAGE_BINDING),
        (VulkanoImageUsage::COLOR_ATTACHMENT, WgpuTextureUsages::RENDER_ATTACHMENT),
    ]
    .into_iter()
    .filter_map(|(v, w)| usage.contains(v).then_some(w))
    .collect()
}

fn vulkano_usage_into_wgpu_hal_usage(usage: VulkanoImageUsage) -> WgpuHalTextureUses {
    [
        (VulkanoImageUsage::TRANSFER_SRC, WgpuHalTextureUses::COPY_SRC),
        (VulkanoImageUsage::TRANSFER_DST, WgpuHalTextureUses::COPY_DST),
        (VulkanoImageUsage::SAMPLED, WgpuHalTextureUses::RESOURCE),
        (VulkanoImageUsage::STORAGE, WgpuHalTextureUses::STORAGE_READ_ONLY),
        (VulkanoImageUsage::COLOR_ATTACHMENT, WgpuHalTextureUses::COLOR_TARGET),
    ]
    .into_iter()
    .filter_map(|(v, w)| usage.contains(v).then_some(w))
    .collect()
}

fn vulkano_usage_into_wgpu_memory_flags(usage: VulkanoImageUsage) -> WgpuHalMemoryFlags {
    [(VulkanoImageUsage::TRANSIENT_ATTACHMENT, WgpuHalMemoryFlags::TRANSIENT)].into_iter().filter_map(|(v, w)| usage.contains(v).then_some(w)).collect()
}

impl<T> eframe::App for MPDeltaEframeApp<T>
where
    T: Gui<ImageType> + 'static,
{
    fn update(&mut self, ctx: &Context, frame: &mut Frame) {
        let render_state = frame.wgpu_render_state().unwrap();
        let device = &render_state.device;
        let mut renderer = render_state.renderer.write();
        let mut image_register = WgpuImageRegister::new(device, &mut renderer);
        self.gui.ui(ctx, &mut image_register);
        ctx.request_repaint();
    }
}
