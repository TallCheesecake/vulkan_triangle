#![allow(
    dead_code,
    unsafe_op_in_unsafe_fn,
    unused_variables,
    clippy::too_many_arguments
)]
use anyhow::{Result, anyhow};
use log::*;
use std::collections::HashSet;
use std::ffi::CStr;
use std::os::raw::c_void;
use thiserror::Error;
use vulkanalia::Version;
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::vk::ExtDebugUtilsExtensionInstanceCommands;
use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;
use vulkanalia::vk::KhrSwapchainExtensionDeviceCommands;
use vulkanalia::window as vk_window;
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};
//nice bool bro
const DEVICE_EXTENSIONS: &[vk::ExtensionName] = &[vk::KHR_SWAPCHAIN_EXTENSION.name];
extern "system" fn debug_callback(
    severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    type_: vk::DebugUtilsMessageTypeFlagsEXT,
    data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _: *mut c_void,
) -> vk::Bool32 {
    let data = unsafe { *data };
    let message = unsafe { CStr::from_ptr(data.message) }.to_string_lossy();

    if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::ERROR {
        error!("({:?}) {}", type_, message);
    } else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::WARNING {
        warn!("({:?}) {}", type_, message);
    } else if severity >= vk::DebugUtilsMessageSeverityFlagsEXT::INFO {
        debug!("({:?}) {}", type_, message);
    } else {
        trace!("({:?}) {}", type_, message);
    }
    vk::FALSE
}

const VALIDATION_ENABLED: bool = cfg!(debug_assertions);
const VALIDATION_LAYER: vk::ExtensionName =
    vk::ExtensionName::from_bytes(b"VK_LAYER_KHRONOS_validation");

fn main() -> Result<()> {
    pretty_env_logger::init();
    let event_loop = EventLoop::new()?;
    let window = WindowBuilder::new()
        .with_title("my window")
        .with_inner_size(LogicalSize::new(800, 900))
        .build(&event_loop)?;
    let mut app = unsafe { App::create(&window)? };
    event_loop.run(|event, elwt| match event {
        Event::AboutToWait => window.request_redraw(),
        Event::WindowEvent { event, .. } => match event {
            WindowEvent::CloseRequested => {
                elwt.exit();
                unsafe {
                    app.destroy();
                }
            }
            WindowEvent::RedrawRequested if !elwt.exiting() => unsafe {
                app.render(&window).unwrap()
            },
            _ => {}
        },

        _ => {}
    })?;
    Ok(())
}
struct App {
    entry: Entry,
    instance: Instance,
    data: AppData,
    device: Device,
}
unsafe fn create_swapchain(
    window: &Window,
    instance: &Instance,
    device: &Device,
    data: &mut AppData,
) -> Result<()> {
    let indices = QueueFamilyIndices::get(instance, data, data.physical_device)?;
    let support = SwapchainSupport::get(instance, data, data.physical_device)?;
    let surface_format = get_swapchain_surface_format(&support.formats);
    let present_mode = get_swapchain_present_mode(&support.present_modes);
    let extent = get_swapchain_extent(window, support.capabilities);

    let mut image_count = support.capabilities.min_image_count + 1;
    if support.capabilities.max_image_count != 0
        && image_count > support.capabilities.max_image_count
    {
        image_count = support.capabilities.max_image_count;
    }
    let mut queue_family_indices = vec![];
    let image_sharing_mode = if indices.graphics != indices.present {
        queue_family_indices.push(indices.graphics);
        queue_family_indices.push(indices.present);
        vk::SharingMode::CONCURRENT
    } else {
        vk::SharingMode::EXCLUSIVE
    };
    //Smallest vulkan struct
    let info = vk::SwapchainCreateInfoKHR::builder()
        .surface(data.surface)
        .min_image_count(image_count)
        .image_format(surface_format.format)
        .image_color_space(surface_format.color_space)
        .image_extent(extent)
        .image_array_layers(1)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(image_sharing_mode)
        .pre_transform(support.capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true)
        .queue_family_indices(&queue_family_indices)
        .old_swapchain(vk::SwapchainKHR::null());
    data.swapchain = device.create_swapchain_khr(&info, None)?;
    data.swapchain_images = device.get_swapchain_images_khr(data.swapchain)?;
    data.swapchain_format = surface_format.format;
    data.swapchain_extent = extent;
    Ok(())
}
impl App {
    unsafe fn create(window: &Window) -> Result<Self> {
        let loader = LibloadingLoader::new(LIBRARY)?;
        let entry = Entry::new(loader).map_err(|e| anyhow!("entry creation err: {}", e))?;
        let mut data = AppData::default();
        let instance = create_instace(window, &entry, &mut data)?;
        data.surface = vk_window::create_surface(&instance, &window, &window)?;
        pick_physical_device(&instance, &mut data)?;
        let device = create_logical_device(&entry, &instance, &mut data)?;
        create_swapchain(&window, &instance, &device, &mut data)?;
        Ok(Self {
            entry,
            instance,
            data,
            device,
        })
    }
    unsafe fn render(&mut self, window: &Window) -> Result<()> {
        Ok(())
    }
    unsafe fn destroy(&mut self) {
        self.device.destroy_swapchain_khr(self.data.swapchain, None);
        self.device.destroy_device(None);
        if VALIDATION_ENABLED {
            self.instance
                .destroy_debug_utils_messenger_ext(self.data.messenger, None);
        }
        self.instance.destroy_surface_khr(self.data.surface, None);
        self.instance.destroy_instance(None);
    }
}

#[derive(Clone, Debug)]
struct SwapchainSupport {
    capabilities: vk::SurfaceCapabilitiesKHR,
    formats: Vec<vk::SurfaceFormatKHR>,
    present_modes: Vec<vk::PresentModeKHR>,
}
impl SwapchainSupport {
    unsafe fn get(
        instance: &Instance,
        AppData: &AppData,
        physical_device: vk::PhysicalDevice,
    ) -> Result<Self> {
        Ok(Self {
            capabilities: instance
                .get_physical_device_surface_capabilities_khr(physical_device, AppData.surface)?,
            formats: instance
                .get_physical_device_surface_formats_khr(physical_device, AppData.surface)?,
            present_modes: instance
                .get_physical_device_surface_present_modes_khr(physical_device, AppData.surface)?,
        })
    }
}
unsafe fn create_logical_device(
    entry: &Entry,
    instance: &Instance,
    data: &mut AppData,
) -> Result<Device> {
    let indicies = QueueFamilyIndices::get(instance, data, data.physical_device)?;
    let mut unique_queue_indicies = HashSet::new();
    unique_queue_indicies.insert(indicies.graphics);
    unique_queue_indicies.insert(indicies.present);
    let queue_priorities = &[1.0];
    // let queue_info = vk::DeviceQueueCreateInfo::builder()
    //     .queue_family_index(indicies.graphics as u32)
    //     .queue_priorities(queue_priorities)
    //     .build();
    let layers = if VALIDATION_ENABLED {
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        vec![]
    };
    let mut extentions = DEVICE_EXTENSIONS
        .iter()
        .map(|i| i.as_ptr())
        .collect::<Vec<_>>();
    if cfg!(target_os = "macos") && entry.version()? >= PORTABILITY_MACOS_VERSION {
        extentions.push(vk::KHR_PORTABILITY_SUBSET_EXTENSION.name.as_ptr());
    }
    let features = vk::PhysicalDeviceFeatures::builder();
    let queue_infos = unique_queue_indicies
        .iter()
        .map(|i| {
            vk::DeviceQueueCreateInfo::builder()
                .queue_priorities(queue_priorities)
                .queue_family_index(*i)
        })
        .collect::<Vec<_>>();
    let info = vk::DeviceCreateInfo::builder()
        .enabled_extension_names(&extentions)
        .enabled_features(&features)
        .enabled_layer_names(&layers)
        .queue_create_infos(&queue_infos)
        .build();
    let device = instance.create_device(data.physical_device, &info, None)?;
    data.graphics_queue = device.get_device_queue(indicies.graphics, 0);
    data.present_queue = device.get_device_queue(indicies.present, 0);
    Ok(device)
}
#[derive(Debug, Error)]
#[error("Missing {0}.")]
pub struct SuitabilityError(pub &'static str);

unsafe fn pick_physical_device(instance: &Instance, data: &mut AppData) -> Result<()> {
    for physical_device in instance.enumerate_physical_devices()? {
        let properties = instance.get_physical_device_properties(physical_device);
        let available_physical_devices = check_physical_device(instance, data, physical_device);
        match available_physical_devices {
            Ok(_) => {
                info!("Selected physical device (`{}`).", properties.device_name);
                data.physical_device = physical_device;
                return Ok(());
            }
            Err(e) => {
                warn!(
                    "Skipping physical device (`{}`): {}",
                    properties.device_name, e
                );
            }
        }
    }
    Err(anyhow!("Failed to find device"))
}

fn get_swapchain_extent(window: &Window, capabilities: vk::SurfaceCapabilitiesKHR) -> vk::Extent2D {
    //Magic numbers for some reason
    if capabilities.current_extent.width != u32::MAX {
        capabilities.current_extent
    } else {
        //Set the max amount it can render to the window size
        vk::Extent2D::builder()
            .width(window.inner_size().width.clamp(
                capabilities.min_image_extent.width,
                capabilities.max_image_extent.width,
            ))
            .height(window.inner_size().height.clamp(
                capabilities.min_image_extent.height,
                capabilities.max_image_extent.height,
            ))
            .build()
    }
}
fn get_swapchain_present_mode(present_modes: &[vk::PresentModeKHR]) -> vk::PresentModeKHR {
    present_modes
        .iter()
        .cloned()
        .find(|i| *i == vk::PresentModeKHR::MAILBOX)
        .unwrap_or(vk::PresentModeKHR::FIFO)
}
fn get_swapchain_surface_format(formats: &[vk::SurfaceFormatKHR]) -> vk::SurfaceFormatKHR {
    formats
        .iter()
        .cloned()
        .find(|i| {
            i.format == vk::Format::B8G8R8_SRGB
                && i.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .unwrap_or_else(|| formats[0])
}

unsafe fn check_physical_device(
    instance: &Instance,
    data: &AppData,
    physical_device: vk::PhysicalDevice,
) -> anyhow::Result<()> {
    QueueFamilyIndices::get(instance, data, physical_device)?;
    let support = SwapchainSupport::get(instance, data, physical_device)?;
    if support.present_modes.is_empty() || support.formats.is_empty() {
        return Err(anyhow!(SuitabilityError("Your swap chain does not work")));
    }
    check_physical_device_extensions(&instance, physical_device)?;
    let properties = instance.get_physical_device_properties(physical_device);
    if properties.device_type != vk::PhysicalDeviceType::DISCRETE_GPU {
        return Err(anyhow!(SuitabilityError("We only support discrete GPUs")));
    }

    let features = instance.get_physical_device_features(physical_device);
    if features.geometry_shader != vk::TRUE {
        return Err(anyhow!(SuitabilityError(
            "You need to have geometry shader support"
        )));
    }
    Ok(())
}
unsafe fn check_physical_device_extensions(
    instance: &Instance,
    physical_device: vk::PhysicalDevice,
) -> Result<()> {
    let extention = instance
        .enumerate_device_extension_properties(physical_device, None)?
        .iter()
        .map(|e| e.extension_name)
        .collect::<HashSet<_>>();
    if DEVICE_EXTENSIONS.iter().all(|i| extention.contains(i)) {
        Ok(())
    } else {
        error!("hello");
        return Err(anyhow!(SuitabilityError(
            "The device does not hvae the needed extentiosn"
        )));
    }
}
#[derive(Clone, Copy, Debug)]
struct QueueFamilyIndices {
    graphics: u32,
    present: u32,
}

impl QueueFamilyIndices {
    fn get(
        instance: &Instance,
        data: &AppData,
        physical_device: vk::PhysicalDevice,
    ) -> Result<Self> {
        let properties =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };

        let graphics = properties
            .iter()
            .position(|i| i.queue_flags.contains(vk::QueueFlags::GRAPHICS));

        let present = unsafe {
            properties
                .iter()
                .enumerate()
                .find_map(|(index, _property)| {
                    if instance
                        .get_physical_device_surface_support_khr(
                            physical_device,
                            index as u32,
                            data.surface,
                        )
                        .ok()?
                    {
                        Some(index as u32)
                    } else {
                        None
                    }
                })
        };

        let pair = (graphics, present);
        match pair {
            (Some(x), Some(y)) => Ok(Self {
                graphics: x as u32,
                present: y as u32,
            }),
            _ => Err(anyhow!(SuitabilityError(
                "The queues on your device are not supported"
            ))),
        }
    }
}

const PORTABILITY_MACOS_VERSION: Version = Version::new(1, 3, 216);
#[derive(Default, Debug)]
struct AppData {
    swapchain_format: vk::Format,
    swapchain_extent: vk::Extent2D,
    swapchain: vk::SwapchainKHR,
    swapchain_images: Vec<vk::Image>,
    messenger: vk::DebugUtilsMessengerEXT,
    physical_device: vk::PhysicalDevice,
    graphics_queue: vk::Queue,
    surface: vk::SurfaceKHR,
    present_queue: vk::Queue,
}

unsafe fn create_instace(window: &Window, entry: &Entry, data: &mut AppData) -> Result<Instance> {
    let application_info = vk::ApplicationInfo::builder()
        .application_name(b"my_first_app\0")
        .application_version(vk::make_version(0, 0, 0))
        .engine_version(vk::make_version(0, 0, 0))
        .engine_name(b"No Engine\0")
        .api_version(vk::make_version(0, 0, 0));
    let available_layers = entry
        .enumerate_instance_layer_properties()?
        .iter()
        .map(|l| l.layer_name)
        .collect::<HashSet<_>>();

    let mut extensions = vk_window::get_required_instance_extensions(&window)
        .iter()
        .map(|e| e.as_ptr())
        .collect::<Vec<_>>();

    if VALIDATION_ENABLED {
        extensions.push(vk::EXT_DEBUG_UTILS_EXTENSION.name.as_ptr());
    }
    if VALIDATION_ENABLED {
        extensions.push(vk::EXT_DEBUG_UTILS_EXTENSION.name.as_ptr());
    }
    let avail_lyr = !available_layers.contains(&VALIDATION_LAYER);
    if VALIDATION_ENABLED && avail_lyr {
        return Err(anyhow!("Validation layer req not supported"));
    }
    let layers = if VALIDATION_ENABLED {
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        Vec::new()
    };
    // Required by Vulkan SDK on macOS since 1.3.216.
    let flags = if cfg!(target_os = "macos") && entry.version()? >= PORTABILITY_MACOS_VERSION {
        info!("Enabling extensions for macOS portability.");
        extensions.push(
            vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_EXTENSION
                .name
                .as_ptr(),
        );
        extensions.push(vk::KHR_PORTABILITY_ENUMERATION_EXTENSION.name.as_ptr());
        vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
    } else {
        vk::InstanceCreateFlags::empty()
    };
    let mut info = vk::InstanceCreateInfo::builder()
        .application_info(&application_info)
        .enabled_layer_names(&layers)
        .enabled_extension_names(&extensions)
        .flags(flags);
    let mut debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
        .user_callback(Some(debug_callback))
        .message_type(
            vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
        )
        .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
        .build();
    if VALIDATION_ENABLED {
        info = info.push_next(&mut debug_info);
    }
    let instance = entry.create_instance(&info, None)?;
    if VALIDATION_ENABLED {
        let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::builder()
            .message_severity(vk::DebugUtilsMessageSeverityFlagsEXT::all())
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .user_callback(Some(debug_callback));
        data.messenger = instance.create_debug_utils_messenger_ext(&debug_info, None)?;
    }
    Ok(instance)
}
