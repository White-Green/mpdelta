use egui::TextureId;
use mpdelta_core_vulkano::ImageType;
use mpdelta_gui::view::Gui;
use mpdelta_gui::ImageRegister;
use std::sync::Arc;
use vulkano::command_buffer::{AutoCommandBufferBuilder, CommandBufferUsage, PrimaryCommandBuffer, RenderPassBeginInfo, SubpassContents};
use vulkano::device::{Device, DeviceExtensions, Queue};
use vulkano::format::{ClearValue, NumericType};
use vulkano::image::view::ImageView;
use vulkano::image::{ImageAccess, ImageUsage, SwapchainImage};
use vulkano::instance::{Instance, InstanceExtensions};
use vulkano::pipeline::graphics::viewport::Viewport;
use vulkano::render_pass::{Framebuffer, FramebufferCreateInfo, RenderPass, Subpass};
use vulkano::swapchain::{AcquireError, FullScreenExclusive, Surface, SurfaceInfo, Swapchain, SwapchainCreateInfo, SwapchainCreationError};
use vulkano::sync::{FlushError, GpuFuture};
use winit::event::{Event, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::Window;

struct ImageRegisterWrapper<'a>(&'a mut egui_winit_vulkano::Gui);

impl<'a> ImageRegister<ImageType> for ImageRegisterWrapper<'a> {
    fn register_image(&mut self, texture: ImageType) -> TextureId {
        self.0.register_user_image_view(ImageView::new_default(texture.0).unwrap())
    }

    fn unregister_image(&mut self, id: TextureId) {
        self.0.unregister_user_image(id);
    }
}

pub struct MPDeltaGUIVulkano<T> {
    gui: T,
    vulkano_gui: egui_winit_vulkano::Gui,
    device: Arc<Device>,
    queue: Arc<Queue>,
    event_loop: EventLoop<()>,
    surface: Arc<Surface<Window>>,
}

pub fn required_extensions() -> InstanceExtensions {
    InstanceExtensions::none()
}

pub fn device_extensions() -> DeviceExtensions {
    DeviceExtensions { khr_swapchain: true, ..DeviceExtensions::none() }
}

impl<T: Gui<ImageType> + 'static> MPDeltaGUIVulkano<T> {
    pub fn new(device: Arc<Device>, queue: Arc<Queue>, event_loop: EventLoop<()>, surface: Arc<Surface<Window>>, gui: T) -> MPDeltaGUIVulkano<T> {
        let vulkano_gui = egui_winit_vulkano::Gui::new(Arc::clone(&surface), None, Arc::clone(&queue), false);
        MPDeltaGUIVulkano { gui, vulkano_gui, device, queue, event_loop, surface }
    }

    pub fn main(self) {
        let MPDeltaGUIVulkano {
            mut gui,
            mut vulkano_gui,
            device,
            queue,
            event_loop,
            surface,
        } = self;
        let (mut swapchain, images) = {
            let physical_device = device.physical_device();
            let caps = physical_device.surface_capabilities(&surface, Default::default()).unwrap();
            let composite_alpha = caps.supported_composite_alpha.iter().next().unwrap();

            let supported_formats = physical_device
                .surface_formats(
                    &surface,
                    SurfaceInfo {
                        full_screen_exclusive: FullScreenExclusive::Default,
                        win32_monitor: None,
                        ..SurfaceInfo::default()
                    },
                )
                .unwrap();
            let (image_format, _) = supported_formats
                .into_iter()
                .max_by_key(|(format, _)| match format.type_color() {
                    Some(NumericType::SRGB) => 1,
                    _ => 0,
                })
                .unwrap();
            let image_extent: [u32; 2] = surface.window().inner_size().into();

            Swapchain::new(
                Arc::clone(&device),
                Arc::clone(&surface),
                SwapchainCreateInfo {
                    min_image_count: caps.min_image_count,
                    image_format: Some(image_format),
                    image_extent,
                    image_usage: ImageUsage::color_attachment(),
                    composite_alpha,

                    ..Default::default()
                },
            )
            .unwrap()
        };

        let mut viewport = Viewport {
            origin: [0.0, 0.0],
            dimensions: [0.0, 0.0],
            depth_range: 0.0..1.0,
        };

        let mut framebuffers = window_size_dependent_setup(&images, &mut viewport);

        let mut recreate_swapchain = false;

        let none_command = AutoCommandBufferBuilder::primary(Arc::clone(&device), queue.family(), CommandBufferUsage::OneTimeSubmit).unwrap().build().unwrap();
        let mut previous_frame_end = none_command.execute(Arc::clone(&queue)).unwrap().boxed().then_signal_fence();

        event_loop.run(move |event, _, control_flow| {
            match event {
                Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                    *control_flow = ControlFlow::Exit;
                }
                Event::WindowEvent { event: WindowEvent::Resized(_), .. } => {
                    recreate_swapchain = true;
                }
                Event::WindowEvent { event, .. } => {
                    let egui_consumed_event = vulkano_gui.update(&event);
                    if !egui_consumed_event {
                        // 必要ならここでイベントハンドリングをする
                    };
                }
                Event::RedrawEventsCleared => {
                    previous_frame_end.cleanup_finished();

                    if recreate_swapchain {
                        let dimensions: [u32; 2] = surface.window().inner_size().into();
                        let (new_swapchain, new_images) = match swapchain.recreate(SwapchainCreateInfo {
                            image_extent: surface.window().inner_size().into(),
                            ..swapchain.create_info()
                        }) {
                            Ok(r) => r,
                            Err(SwapchainCreationError::ImageExtentNotSupported { .. }) => return,
                            Err(e) => panic!("Failed to recreate swapchain: {:?}", e),
                        };

                        swapchain = new_swapchain;
                        framebuffers = window_size_dependent_setup(&new_images, &mut viewport);
                        viewport.dimensions = [dimensions[0] as f32, dimensions[1] as f32];
                        recreate_swapchain = false;
                    }

                    let (image_num, suboptimal, acquire_future) = match vulkano::swapchain::acquire_next_image(swapchain.clone(), None) {
                        Ok(r) => r,
                        Err(AcquireError::OutOfDate) => {
                            recreate_swapchain = true;
                            return;
                        }
                        Err(e) => panic!("Failed to acquire next image: {:?}", e),
                    };

                    if suboptimal {
                        recreate_swapchain = true;
                    }

                    vulkano_gui.begin_frame();
                    gui.ui(&vulkano_gui.context(), &mut ImageRegisterWrapper(&mut vulkano_gui));

                    take_mut::take(&mut previous_frame_end, |previous_frame_end| {
                        previous_frame_end.wait(None).unwrap();
                        let future = vulkano_gui.draw_on_image(acquire_future, Arc::clone(&framebuffers[image_num]) as Arc<_>);
                        let future = future.then_swapchain_present(Arc::clone(&queue), Arc::clone(&swapchain), image_num).boxed().then_signal_fence_and_flush();

                        match future {
                            Ok(future) => future,
                            Err(FlushError::OutOfDate) => {
                                recreate_swapchain = true;
                                vulkano::sync::now(Arc::clone(&device)).boxed().then_signal_fence()
                            }
                            Err(e) => {
                                println!("Failed to flush future: {:?}", e);
                                vulkano::sync::now(Arc::clone(&device)).boxed().then_signal_fence()
                            }
                        }
                    });
                }
                _ => (),
            }
        });
    }
}

fn window_size_dependent_setup(images: &[Arc<SwapchainImage<Window>>], viewport: &mut Viewport) -> Vec<Arc<ImageView<SwapchainImage<Window>>>> {
    let dimensions = images[0].dimensions().width_height();
    viewport.dimensions = [dimensions[0] as f32, dimensions[1] as f32];

    images.iter().map(|image| ImageView::new_default(image.clone()).unwrap()).collect::<Vec<_>>()
}
