use std::collections::HashSet;

use anyhow::{anyhow, Result};
use log::*;
use winit::window::Window;
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::window as vk_window;
use vulkanalia::vk::KhrSurfaceExtensionInstanceCommands;

use crate::SuitabilityError;
use crate::consts::{PORTABILITY_MACOS_VERSION, VALIDATION_ENABLED, VALIDATION_LAYER};


/*
 * --- App Data ---
 */

/// Our Vulkan app.
#[derive(Clone, Debug)]
pub struct App {
    entry: Entry,
    instance: Instance,
    data : AppData,
    device : Device,
}

impl App {
    /// Creates our Vulkan app.
    pub unsafe fn create(window: &Window) -> Result<Self> {
        let loader = LibloadingLoader::new(LIBRARY)?;
        let entry = Entry::new(loader).map_err(|b| anyhow!("{}", b))?;
        let instance = create_instance(window, &entry)?;
        let mut data = AppData::default();
        data.surface = vk_window::create_surface(&instance, &window, &window)?;
        pick_physical_device(&instance, &mut data)?;
        let device = create_logical_device(&entry, &instance, &mut data)?;
        Ok( Self { entry, instance, data, device })
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(&mut self, window: &Window) -> Result<()> {
        Ok(())
    }

    /// Destroys our Vulkan app.
    pub unsafe fn destroy(&mut self) {
        self.device.destroy_device(None);
        // window handle
        self.instance.destroy_surface_khr(self.data.surface, None);
        // 9/10 moet dit als laatsts gedestroyed worden
        self.instance.destroy_instance(None);
    }
}

/*
 * --- App Data ---
 */
/// The Vulkan handles and associated properties used by our Vulkan app.
#[derive(Clone, Debug, Default)]
pub struct AppData {
    physical_device: vk::PhysicalDevice,
    graphics_queue : vk::Queue,
    /// queue for window surface cmds
    present_queue : vk::Queue,
    surface : vk::SurfaceKHR,

}

/*
 * --- Queue Family Indices ---
 */
#[derive(Copy, Clone, Debug)]
struct QueueFamilyIndices {
    graphics : u32,
    present : u32,
}
impl QueueFamilyIndices {
    pub unsafe fn get(
        instance : &Instance,
        data : &AppData,
        p_device : vk::PhysicalDevice
        ) -> Result<Self> {
        let properties = instance.get_physical_device_queue_family_properties(p_device);
        
        // aparte queue voor window presentation
        let mut present = None;
        for (i, props) in properties.iter().enumerate() {
            if instance.get_physical_device_surface_support_khr(p_device, i as u32, data.surface)? {
                present = Some(i as u32);
                break;
            }
        }

        let graphics = properties
            .iter()
            .position(|p| p.queue_flags.contains(vk::QueueFlags::GRAPHICS))
            .map(|i| i as u32);

        if let (Some(graphics), Some(present)) = (graphics, present) {
            Ok( Self { graphics, present })
        } else {
            Err(anyhow!(SuitabilityError("Missing required queue families.")))
        }
    }
}

/*
 * ------
 *
 * PRIVATE FUNCTIONS
 *
 * ------
 */


unsafe fn create_instance(window: &Window, entry: &Entry) -> Result<Instance> {
    // Application Info

    let application_info = vk::ApplicationInfo::builder()
        .application_name(b"Vulkan Tutorial (Rust)\0")
        .application_version(vk::make_version(1, 0, 0))
        .engine_name(b"No Engine\0")
        .engine_version(vk::make_version(1, 0, 0))
        .api_version(vk::make_version(1, 0, 0));

    // Extensions

    let mut extensions = vk_window::get_required_instance_extensions(window)
        .iter()
        .map(|e| e.as_ptr())
        .collect::<Vec<_>>();

    // Required by Vulkan SDK on macOS since 1.3.216.
    let flags = if cfg!(target_os = "macos") && entry.version()? >= PORTABILITY_MACOS_VERSION {
        info!("Enabling extensions for macOS portability.");
        extensions.push(vk::KHR_GET_PHYSICAL_DEVICE_PROPERTIES2_EXTENSION.name.as_ptr());
        extensions.push(vk::KHR_PORTABILITY_ENUMERATION_EXTENSION.name.as_ptr());
        vk::InstanceCreateFlags::ENUMERATE_PORTABILITY_KHR
    } else {
        vk::InstanceCreateFlags::empty()
    };

    // Check for validation
    
    let all_layers = entry
        .enumerate_instance_layer_properties()?
        .iter()
        .map(|l| l.layer_name)
        .collect::<HashSet<_>>();
    if VALIDATION_ENABLED && !all_layers.contains(&VALIDATION_LAYER) {
        return Err(anyhow!("Validation requested but not supported."))
    }  
    let layers = if VALIDATION_ENABLED {
        info!("Validation is enabled.");
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        Vec::new()
    };

    // Create

    let info = vk::InstanceCreateInfo::builder()
        .application_info(&application_info)
        .enabled_extension_names(&extensions)
        .enabled_layer_names(&layers)
        .flags(flags);

    Ok(entry.create_instance(&info, None)?)
}

unsafe fn pick_physical_device(instance : &Instance, data : &mut AppData) -> Result<()> {
    for device in instance.enumerate_physical_devices()? {
        let props = instance.get_physical_device_properties(device);

        if let Err(error) = check_physical_device(instance, data, device) {
            warn!("Skipping physical_device (`{}`): {}", props.device_name, error)
        } else {
            info!("Selecting physical dvice (`{}`)", props.device_name);
            data.physical_device = device;
            return Ok(())
        }
    }
    Err(anyhow!("Failed to find suitable physical device"))
}
unsafe fn check_physical_device(
    instance : &Instance,
    data : &AppData,
    p_device : vk::PhysicalDevice
) -> Result<()> {
    // name, type, supported vulkan vers
    let props = instance.get_physical_device_properties(p_device);
    // ex
    // if props.device_type != vk::PhysicalDeviceType::DISCRETE_GPU {
    //     return Err(anyhow!(SuitabilityError("Only discrete GPUs are supported.")));
    // }
    
    // opt feats like texture compress, 64b floats, multi-viewport rendering
    let feats = instance.get_physical_device_features(p_device);
    // ex
    // if feats.geometry_shader != vk::TRUE {
    //     return Err(anyhow!(SuitabilityError("Missing geometry shader support.")))
    // }
    QueueFamilyIndices::get(instance, data, p_device)?;
    Ok(())
}



unsafe fn create_logical_device(
    entry : &Entry,
    instance : &Instance,
    data : &mut AppData,
    ) -> Result<Device> {

    let indices = QueueFamilyIndices::get(instance, data, data.physical_device)?;
    let mut unique_indices = HashSet::new();
    unique_indices.insert(indices.graphics);
    unique_indices.insert(indices.present);

    let queue_prios = &[1.0];
    let queue_infos = unique_indices
        .iter()
        .map(|i| {
            vk::DeviceQueueCreateInfo::builder()
                .queue_family_index(*i)
                .queue_priorities(queue_prios)
        })
    .collect::<Vec<_>>();


    // welke layers enablen. Die names worden geskipped in moderne versies, maar backwards compat
    // met oude versies is altijd een goei idee dus we setten ze wel
    let layers = if VALIDATION_ENABLED {
        vec![VALIDATION_LAYER.as_ptr()]
    } else {
        vec![]
    };

    let mut ext = vec![];
    // Required by Vulkan SDK on macOS since 1.3.216.
    if cfg!(target_os = "macos") && entry.version()? >= PORTABILITY_MACOS_VERSION {
        ext.push(vk::KHR_PORTABILITY_SUBSET_EXTENSION.name.as_ptr());
    }

    // default vanalles op 'false'. Enable dingen als ge ze nodig hebt
    let feats = vk::PhysicalDeviceFeatures::builder();

    // let queue_infos = &[queue_infos];
    let info = vk::DeviceCreateInfo::builder()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&ext)
        .enabled_features(&feats)
        .enabled_layer_names(&layers);

    let device = instance.create_device(data.physical_device, &info, None)?;
    data.graphics_queue = device.get_device_queue(indices.graphics, 0);
    data.present_queue = device.get_device_queue(indices.present, 0);

    Ok(device)
}
