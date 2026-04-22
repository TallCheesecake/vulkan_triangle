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
use vulkanalia::vk::MVK_MACOS_SURFACE_EXTENSION;
use vulkanalia::window as vk_window;
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::{Window, WindowBuilder};
//nice bool bro
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

impl App {
    unsafe fn create(window: &Window) -> Result<Self> {
        let loader = LibloadingLoader::new(LIBRARY)?;
        let entry = Entry::new(loader).map_err(|e| anyhow!("entry creation err: {}", e))?;
        let mut data = AppData::default();
        let instance = create_instace(window, &entry, &mut data)?;
        pick_physical_device(&instance, &mut data)?;
        let device = create_logical_device(&entry, &instance, &mut data)?;
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
        self.device.destroy_device(None);
        if VALIDATION_ENABLED {
            self.instance
                .destroy_debug_utils_messenger_ext(self.data.messenger, None);
        }
        self.instance.destroy_instance(None);
    }
}

unsafe fn create_logical_device(
    entry: &Entry,
    instance: &Instance,
    data: &mut AppData,
) -> Result<Device> {
    let indicies = QueueFamilyIndices::get(instance, data, data.physical_device)?;
    let queue_priorities = &[1.0];
    let queue_info = vk::DeviceQueueCreateInfo::builder()
        .queue_family_index(indicies.graphics as u32)
        .queue_priorities(queue_priorities)
        .build();
    let layers = if VALIDATION_ENABLED {
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        vec![]
    };
    let mut extentions = vec![];
    if cfg!(target_os = "macos") && entry.version()? >= PORTABILITY_MACOS_VERSION {
        extentions.push(vk::KHR_PORTABILITY_SUBSET_EXTENSION.name.as_ptr());
    }
    let features = vk::PhysicalDeviceFeatures::builder();
    let queue_infos = &[queue_info];
    let info = vk::DeviceCreateInfo::builder()
        .enabled_extension_names(&extentions)
        .enabled_features(&features)
        .enabled_layer_names(&layers)
        .queue_create_infos(queue_infos)
        .build();
    let device = instance.create_device(data.physical_device, &info, None)?;
    //first one? yeah because it only returns the firsyt one
    data.graphics_queue = device.get_device_queue(indicies.graphics, 0);
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
unsafe fn check_physical_device(
    instance: &Instance,
    data: &AppData,
    physical_device: vk::PhysicalDevice,
) -> anyhow::Result<()> {
    QueueFamilyIndices::get(instance, data, physical_device)?;
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

#[derive(Clone, Copy, Debug)]
struct QueueFamilyIndices {
    graphics: u32,
}

impl QueueFamilyIndices {
    fn get(
        instance: &Instance,
        data: &AppData,
        physical_device: vk::PhysicalDevice,
    ) -> Result<Self> {
        let properties =
            unsafe { instance.get_physical_device_queue_family_properties(physical_device) };
        //we need at least one
        let graphics = properties
            .iter()
            .position(|i| i.queue_flags.contains(vk::QueueFlags::GRAPHICS));
        match graphics {
            Some(x) => Ok(Self { graphics: x as u32 }),
            None => Err(anyhow!(SuitabilityError("you dont have a graphics queue"))),
        }
    }
}

const PORTABILITY_MACOS_VERSION: Version = Version::new(1, 3, 216);
#[derive(Default, Debug)]
struct AppData {
    messenger: vk::DebugUtilsMessengerEXT,
    physical_device: vk::PhysicalDevice,
    graphics_queue: vk::Queue,
    surface: vk::SurfaceKHR,
    //TODO: Make this fance surface obj
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
