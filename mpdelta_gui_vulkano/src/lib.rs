use egui::TextureId;
use egui_winit_vulkano::GuiConfig;
use mpdelta_core_vulkano::ImageType;
use mpdelta_gui::view::Gui;
use mpdelta_gui::ImageRegister;
use std::sync::Arc;
use vulkano::device::DeviceExtensions;
use vulkano::format::Format;
use vulkano::image::sampler::SamplerCreateInfo;
use vulkano::image::view::ImageView;
use vulkano::instance::InstanceExtensions;
use vulkano_util::context::VulkanoContext;
use vulkano_util::window::{VulkanoWindows, WindowDescriptor};
use winit::dpi::PhysicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};

struct ImageRegisterWrapper<'a>(&'a mut egui_winit_vulkano::Gui);

impl<'a> ImageRegister<ImageType> for ImageRegisterWrapper<'a> {
    fn register_image(&mut self, texture: ImageType) -> TextureId {
        self.0.register_user_image_view(ImageView::new_default(texture.0).unwrap(), SamplerCreateInfo::simple_repeat_linear_no_mipmap())
    }

    fn unregister_image(&mut self, id: TextureId) {
        self.0.unregister_user_image(id);
    }
}

pub struct MPDeltaGUIVulkano<T> {
    gui: T,
    context: Arc<VulkanoContext>,
    event_loop: EventLoop<()>,
    windows: VulkanoWindows,
}

pub fn required_extensions() -> InstanceExtensions {
    InstanceExtensions::empty()
}

pub fn device_extensions() -> DeviceExtensions {
    DeviceExtensions { khr_swapchain: true, ..DeviceExtensions::empty() }
}

impl<T: Gui<ImageType> + 'static> MPDeltaGUIVulkano<T> {
    pub fn new(context: Arc<VulkanoContext>, event_loop: EventLoop<()>, windows: VulkanoWindows, gui: T) -> MPDeltaGUIVulkano<T> {
        MPDeltaGUIVulkano { gui, context, event_loop, windows }
    }

    pub fn main(self) {
        let MPDeltaGUIVulkano { mut gui, context, event_loop, mut windows } = self;

        // TODO: DeviceによってサポートされているFormatを選ぶようなコードにしたほうがいいかもしれない
        let window = windows.create_window(&event_loop, &context, &WindowDescriptor::default(), |sc| sc.image_format = Format::B8G8R8A8_UNORM);

        {
            let window = windows.get_window(window).unwrap();
            window.set_title("mpdelta");
            window.set_inner_size(PhysicalSize::new(1_920, 1_080));
        }
        let renderer = windows.get_renderer_mut(window).unwrap();
        let mut vulkano_gui = egui_winit_vulkano::Gui::new(&event_loop, renderer.surface(), Arc::clone(context.graphics_queue()), Format::B8G8R8A8_UNORM, GuiConfig::default());

        gui.init(&vulkano_gui.context());

        event_loop.run(move |event, _, control_flow| {
            let renderer = windows.get_renderer_mut(window).unwrap();
            match event {
                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                    *control_flow = ControlFlow::Exit;
                }
                Event::WindowEvent {
                    event: WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. },
                    ..
                } => {
                    renderer.resize();
                }
                Event::MainEventsCleared => {
                    renderer.window().request_redraw();
                }
                Event::WindowEvent { event, .. } => {
                    let egui_consumed_event = vulkano_gui.update(&event);
                    if !egui_consumed_event {
                        // 必要ならここでイベントハンドリングをする
                    };
                }
                Event::RedrawRequested(id) if id == window => {
                    vulkano_gui.begin_frame();
                    gui.ui(&vulkano_gui.context(), &mut ImageRegisterWrapper(&mut vulkano_gui));
                    let before_future = renderer.acquire().unwrap();
                    let after_future = vulkano_gui.draw_on_image(before_future, renderer.swapchain_image_view());
                    renderer.present(after_future, true);
                }
                _ => {}
            }
        });
    }
}
