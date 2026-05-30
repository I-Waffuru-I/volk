use std::collections::HashSet;

use anyhow::{anyhow, Result};
use log::*;
use winit::window::Window;
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::window as vk_window;
use vulkanalia::vk::{KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands, Queue};

use crate::SuitabilityError;
use crate::consts::{DEVICE_EXTENSIONS, PORTABILITY_MACOS_VERSION, VALIDATION_ENABLED, VALIDATION_LAYER};


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

        create_swapchain(&window, &instance, &device, &mut data)?;

        Ok( Self { entry, instance, data, device })
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(&mut self, window: &Window) -> Result<()> {
        Ok(())
    }

    /// Destroys our Vulkan app.
    pub unsafe fn destroy(&mut self) {
        self.device.destroy_swapchain_khr(self.data.swapchain, None);
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
    swapchain : vk::SwapchainKHR,
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


#[derive(Clone, Debug)]
struct SwapchainSupport {
    capabilities : vk::SurfaceCapabilitiesKHR,
    /// Surface format (rgba format & colorspace)
    formats: Vec<vk::SurfaceFormatKHR>,
    /// important, conditions for showing images to the screen
    present_modes: Vec<vk::PresentModeKHR>,
}
impl SwapchainSupport {
    unsafe fn get(
        instance : &Instance,
        data : &AppData,
        p_device : vk::PhysicalDevice,
        ) -> Result<Self> {
        let capabilities = instance.get_physical_device_surface_capabilities_khr(p_device, data.surface)?; 
        let formats = instance.get_physical_device_surface_formats_khr(p_device, data.surface)?;
        let present_modes = instance.get_physical_device_surface_present_modes_khr(p_device, data.surface)?;
        Ok(Self {
            capabilities,
            formats,
            present_modes
        })

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
    check_physical_device_extensions(&instance, p_device)?;
    QueueFamilyIndices::get(instance, data, p_device)?;

    let support = SwapchainSupport::get(&instance, &data, p_device)?;
    if support.formats.is_empty() || support.present_modes.is_empty() {
        return Err(anyhow!(SuitabilityError("Insufficient swapchain support.")))
    }
    Ok(())
}
unsafe fn check_physical_device_extensions(
    instance : &Instance,
    p_device : vk::PhysicalDevice,
) -> Result<()> {
    let ext = instance
        .enumerate_device_extension_properties(p_device, None)?
        .iter()
        .map(|e| e.extension_name )
        .collect::<HashSet<_>>();

    if DEVICE_EXTENSIONS.iter().all(|e| ext.contains(e) ) {
        Ok(())
    } else {
        Err(anyhow!(SuitabilityError("Missing required device extensions")))
    }
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
        // geeft warning at runtime, dus ignore
    // let layers = if VALIDATION_ENABLED {
    //     vec![VALIDATION_LAYER.as_ptr()]
    // } else {
    //     vec!(); 
    // };

    let mut ext = DEVICE_EXTENSIONS
        .iter()
        .map(|n| n.as_ptr())
        .collect::<Vec<_>>();

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
        // gives warning for disabled features
        // .enabled_layer_names(&layers)
        .enabled_features(&feats);


    let device = instance.create_device(data.physical_device, &info, None)?;
    data.graphics_queue = device.get_device_queue(indices.graphics, 0);
    data.present_queue = device.get_device_queue(indices.present, 0);

    Ok(device)
}


unsafe fn create_swapchain(
    window: &Window,
    instance : &Instance,
    device : &Device,
    data : &mut AppData,
    ) -> Result<()> {

    let indices = QueueFamilyIndices::get(&instance, &data, data.physical_device)?;
    let support = SwapchainSupport::get(&instance, &data, data.physical_device)?;

    let surface_format = get_swapchain_surface_format(&support.formats);
    let present_mode = get_swapchain_present_mode(&support.present_modes);
    let extent = get_swapchain_extent(window, support.capabilities);

    let mut image_count = support.capabilities.min_image_count + 1;
    if support.capabilities.max_image_count != 0 
        && image_count > support.capabilities.max_image_count {
            image_count = support.capabilities.max_image_count;
    }

    // define how to handle swapchain imgs that are used across multiple queue families.
    // can happen if graphics queue != presentation queue
    // draw on graphics then submit to pres
    // sharingmode::EXLUSIVE : image is owned by the queue family, have to transfer it first
    // sharingmode::CONCURRENT : can be used across queue fams without ownership transfer
    let mut queue_family_indices = vec![];
    let img_sharing_mode = if indices.graphics != indices.present {
        queue_family_indices.push(indices.graphics);
        queue_family_indices.push(indices.present);
        vk::SharingMode::CONCURRENT
    } else {
        vk::SharingMode::EXCLUSIVE
    };

    let info = vk::SwapchainCreateInfoKHR::builder()
        .surface(data.surface)
        .min_image_count(image_count)
        .image_format(surface_format.format)
        .image_color_space(surface_format.color_space)
        .image_extent(extent)
        // 1, unless making stereoscopic 3D apps
        .image_array_layers(1)
        // what kind of operations the sc will be used for
        // we'll render directly, so color attachment
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(img_sharing_mode)
        .queue_family_indices(&queue_family_indices)
        // if a transform needs to be applied, e.g. a 90° rotation
        // leave at default
        .pre_transform(support.capabilities.current_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        // ignore pixels obstructed by eg another window
        // turn off if you want to read the val of these pixels, but eh, less performance
        .clipped(true)
        // for now, assume only 1 swapchain, but *may* need to be replaced
        .old_swapchain(vk::SwapchainKHR::null());

    data.swapchain = device.create_swapchain_khr(&info, None)?;
    info!("Setup swapchain in AppData.");
    Ok(())
}


fn get_swapchain_surface_format(
    formats : &[vk::SurfaceFormatKHR],
    ) -> vk::SurfaceFormatKHR {
    formats
        .iter()
        .cloned()
        .find(|f| {
            f.format == vk::Format::B8G8R8A8_SRGB 
                && f.color_space == vk::ColorSpaceKHR::SRGB_NONLINEAR
        })
        .unwrap_or_else(|| formats[0])
}
fn get_swapchain_present_mode(
    present_modes : &[vk::PresentModeKHR],
) -> vk::PresentModeKHR {
    // mailbox replaced frames if buffer is full, so is pretty nice
    // fifo as default, which is guaranteed to exist
    present_modes
        .iter()
        .cloned()
        .find(|m| *m == vk::PresentModeKHR::MAILBOX)
        .unwrap_or(vk::PresentModeKHR::FIFO)
}
fn get_swapchain_extent(
    window : &Window,
    capabilities : vk::SurfaceCapabilitiesKHR,
) -> vk::Extent2D {
    // extent is bena altijd de window size 
    // possible om custom te zetten, aangegeven met u32.MAX
    // if so, pak resolutie die best past in min/max_image_extent
    if capabilities.current_extent.width != u32::MAX {
        capabilities.current_extent
    } else {
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

