use mpdelta_gui::view::MPDeltaGUI;
use mpdelta_gui_vulkano::MPDeltaGUIVulkano;
use std::sync::Arc;
use vulkano::device::physical::{PhysicalDevice, PhysicalDeviceType};
use vulkano::device::{Device, DeviceCreateInfo, Queue, QueueCreateInfo};
use vulkano::instance::{Instance, InstanceCreateInfo};
use vulkano::swapchain::Surface;
use vulkano_win::VkSurfaceBuild;
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};

fn main() {
    let gui = MPDeltaGUI::new();
    let (instance, device, queue, event_loop, surface) = initialize_graphics();
    let gui = MPDeltaGUIVulkano::new(instance, device, queue, event_loop, surface, gui);
    gui.main();
}

fn initialize_graphics() -> (Arc<Instance>, Arc<Device>, Arc<Queue>, EventLoop<()>, Arc<Surface<Window>>) {
    let required_extensions = vulkano_win::required_extensions().union(&mpdelta_gui_vulkano::required_extensions());
    let instance = Instance::new(InstanceCreateInfo {
        enabled_extensions: required_extensions,
        ..Default::default()
    })
    .unwrap();

    let event_loop = EventLoop::new();
    let surface = WindowBuilder::new().with_title("mpdelta").build_vk_surface(&event_loop, Arc::clone(&instance)).unwrap();

    let device_extensions = mpdelta_gui_vulkano::device_extensions();

    let (physical_device, queue_family) = PhysicalDevice::enumerate(&instance)
        .filter(|&p| p.supported_extensions().is_superset_of(&device_extensions))
        .filter_map(|p| p.queue_families().find(|&q| q.supports_graphics() && q.supports_surface(&surface).unwrap_or(false)).map(|q| (p, q)))
        .min_by_key(|(p, _)| match p.properties().device_type {
            PhysicalDeviceType::DiscreteGpu => 0,
            PhysicalDeviceType::IntegratedGpu => 1,
            PhysicalDeviceType::VirtualGpu => 2,
            PhysicalDeviceType::Cpu => 3,
            PhysicalDeviceType::Other => 4,
        })
        .unwrap();

    let (device, mut queues) = Device::new(
        physical_device,
        DeviceCreateInfo {
            enabled_extensions: device_extensions,
            queue_create_infos: vec![QueueCreateInfo::family(queue_family)],
            ..Default::default()
        },
    )
    .unwrap();

    let queue = queues.next().unwrap();
    (instance, device, queue, event_loop, surface)
}
