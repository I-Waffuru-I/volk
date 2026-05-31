use std::collections::HashSet;

use anyhow::{anyhow, Result};
use log::*;
use vulkanalia::bytecode::Bytecode;
use winit::window::Window;
use vulkanalia::loader::{LIBRARY, LibloadingLoader};
use vulkanalia::prelude::v1_0::*;
use vulkanalia::window as vk_window;
use vulkanalia::vk::{KhrSurfaceExtensionInstanceCommands, KhrSwapchainExtensionDeviceCommands};

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
        create_swapchain_image_views(&device, &mut data)?;

        create_render_pass(&device, &instance, &mut data)?;
        create_pipeline(&device, &mut data)?;

        create_framebuffers(&device, &mut data)?;

        Ok( Self { entry, instance, data, device })
    }

    /// Renders a frame for our Vulkan app.
    pub unsafe fn render(&mut self, window: &Window) -> Result<()> {
        Ok(())
    }

    /// Destroys our Vulkan app.
    pub unsafe fn destroy(&mut self) {
        self.device.destroy_pipeline( self.data.pipeline, None);
        self.device.destroy_pipeline_layout(self.data.pipeline_layout, None);
        self.data.framebuffers
            .iter()
            .for_each(|f| self.device.destroy_framebuffer(*f, None));
        self.device.destroy_render_pass(self.data.render_pass, None);
        self.data.swapchain_image_views
            .iter()
            .for_each(|i| self.device.destroy_image_view(*i, None));
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
    swapchain_images : Vec<vk::Image>,
    swapchain_format : vk::Format,
    swapchain_extent : vk::Extent2D,
    /// views om images in te renderen
    swapchain_image_views : Vec<vk::ImageView>,
    render_pass: vk::RenderPass,
    pipeline_layout : vk::PipelineLayout,
    pipeline: vk::Pipeline,
    framebuffers : Vec<vk::Framebuffer>,
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


unsafe fn create_framebuffers(
    device : &Device,
    data : &mut AppData
) -> Result<()> {
    data.framebuffers = data
        .swapchain_image_views
        .iter()
        .map(|i| {
            let attachments = &[*i];
            let create_info = vk::FramebufferCreateInfo::builder()
                .attachments(attachments)
                .render_pass(data.render_pass)
                .width(data.swapchain_extent.width)
                .height(data.swapchain_extent.height)
                .layers(1);

            device.create_framebuffer(&create_info, None)
        })
    .collect::<Result<Vec<_>, _>>()?;
    Ok(())
}

unsafe fn create_pipeline(
    device : &Device,
    data : &mut AppData,
) -> Result<()> {
    let vert = include_bytes!("../shaders/vert.spv");
    let frag = include_bytes!("../shaders/frag.spv");

    let vert_module = create_shader_module(device, vert)?;
    let frag_module = create_shader_module(device, frag)?;

    let vert_stage = vk::PipelineShaderStageCreateInfo::builder()
        // welke stage in de pipeline het wordt gebruikt
        .stage(vk::ShaderStageFlags::VERTEX)
        // gebruikt om constants te definieren
        // .specialization_info(specialization_info)
        .module(vert_module)
        // name van de entry point in shader code. Moet niet persé main zijn
        // zo kunt ge dubbele behaviour fixen met 1 enkele shader module bvb
        .name(b"main\0");
    let frag_stage = vk::PipelineShaderStageCreateInfo::builder()
        .stage(vk::ShaderStageFlags::FRAGMENT)
        .module(frag_module)
        .name(b"main\0");

    // default for now want we laden geen vertex data in 
    let vertex_input_state = vk::PipelineVertexInputStateCreateInfo::builder();
        // spacing tussen data, en of het per-vertex of per-instance is 
        // .vertex_binding_descriptions(vertex_binding_descriptions);
        // type attributes gegeven aan de vert shader, welke bindings en welke offset
        // .vertex_attribute_descriptions(vertex_attribute_descriptions)
       

    let input_assembly_state = vk::PipelineInputAssemblyStateCreateInfo::builder()
        // wat voor geometry getekend wordt 
        // POINT_LIST: points from vertices 
        // LINE_LIST: line voor elke 2 vertices, zonder reuse 
        // LINE_STRIP: einde van elke vertex is start voor volgende 
        // TRIANGLE_LIST: triangle van elke 3 vertices, zonder reuse
        // TRIANGLE_STRIP: 2nd en 3e vertex van elke triangle zijn de eerste 2 voor dee volgende
        .topology(vk::PrimitiveTopology::TRIANGLE_LIST)
        // allowed om in _STRIP modes een 0xFFFF index te zetten om clean reset te doen
        .primitive_restart_enable(false);

    // welke regio van de framebuffer de output te renderen.
    // Bena altijd  (0, 0) -> (width, height)
    let viewport = vk::Viewport::builder()
        .x(0.0)
        .y(0.0)
        .width(data.swapchain_extent.width as f32)
        .height(data.swapchain_extent.height as f32)
        .min_depth(0.0)
        .max_depth(1.0);
    // scissors zijn een... filter voor waar op de viewport pixels zetten?
    // idk, gwn default volledige framebuffer pakken ig
    let scissor = vk::Rect2D::builder()
        .offset(vk::Offset2D {x:0, y:0})
        .extent(data.swapchain_extent);
    let viewports = &[viewport];
    let scissors = &[scissor];
    let viewport_state = vk::PipelineViewportStateCreateInfo::builder()
        .viewports(viewports)
        .scissors(scissors);

    let rasterization_state = vk::PipelineRasterizationStateCreateInfo::builder()
        // als true, frags buiten de 'far plane' worden geclamped en niet discarded.
        // handig voor shadow maps. Heeft GPU feature nodig
        .depth_clamp_enable(false)
        // basically turned heel dit off
        // geen geometry passed door de rasterizer en naar framebuffer
        .rasterizer_discard_enable(false)
        // FILL: vul de area van een polygon met frags
        // LINE: teken edges enkel
        // POINT: teken vertices as punten
        .polygon_mode(vk::PolygonMode::FILL)
        // duh, param is width in pixels 
        .line_width(1.0)
        // cull backside frags
        .cull_mode(vk::CullModeFlags::BACK)
        // welke orientation gebruiken om de front face te vinden
        .front_face(vk::FrontFace::CLOCKWISE)
        // alter depth door een value te adden. Kan gebruikt worden in shadow mapping
        .depth_bias_enable(false);

    // een manier om anti-aliasing te doen. GPU feature nodig
    // disable for now
    let multisample_state = vk::PipelineMultisampleStateCreateInfo::builder()
        .sample_shading_enable(false)
        .rasterization_samples(vk::SampleCountFlags::_1);

    // colour blending is het samenvoegen van frag kleur met de kleur die er al was
    // dit mixed old en new colour om een final te berekenen
    let attachment = vk::PipelineColorBlendAttachmentState::builder()
        .color_write_mask(vk::ColorComponentFlags::all())
        // skipped all this, gebruikt new frag colour as final colour
        .blend_enable(false)
        .src_color_blend_factor(vk::BlendFactor::ONE)  
        .dst_color_blend_factor(vk::BlendFactor::ZERO) 
        .color_blend_op(vk::BlendOp::ADD)              
        .src_alpha_blend_factor(vk::BlendFactor::ONE)  
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO) 
        .alpha_blend_op(vk::BlendOp::ADD);             
    /*
     let attachment = vk::PipelineColorBlendAttachmentState::builder()
        .color_write_mask(vk::ColorComponentFlags::all())
        .blend_enable(true)
        .src_color_blend_factor(vk::BlendFactor::SRC_ALPHA)
        .dst_color_blend_factor(vk::BlendFactor::ONE_MINUS_SRC_ALPHA)
        .color_blend_op(vk::BlendOp::ADD)
        .src_alpha_blend_factor(vk::BlendFactor::ONE)
        .dst_alpha_blend_factor(vk::BlendFactor::ZERO)
        .alpha_blend_op(vk::BlendOp::ADD);
     */
    let attachments = &[attachment];
    let color_blend_state = vk::PipelineColorBlendStateCreateInfo::builder()
        // op TRUE zetten als ge colour blend met bitwise operations wil doen
        // zet blend_enable automatisch op false
        .logic_op_enable(false)
        .logic_op(vk::LogicOp::COPY)
        .blend_constants([0.0, 0.0, 0.0, 0.0])
        .attachments(attachments);

    let layout_info = vk::PipelineLayoutCreateInfo::builder();

    data.pipeline_layout = device.create_pipeline_layout(&layout_info, None)?;

    let stages = &[vert_stage, frag_stage];
    let graphics_info = vk::GraphicsPipelineCreateInfo::builder()
        // editable stages 
        .stages(stages)
        // structures die fixed stages beschrijven
        .vertex_input_state(&vertex_input_state)
        .input_assembly_state(&input_assembly_state)
        .viewport_state(&viewport_state)
        .rasterization_state(&rasterization_state)
        .multisample_state(&multisample_state)
        .color_blend_state(&color_blend_state)
        // layout
        .layout(data.pipeline_layout)
        // ref to render pass and index of subpass where the graphics pipeline is used
        .render_pass(data.render_pass)
        .subpass(0)
        // optional, ge kunt afleiden van een andere pipeline
        // switching met inheritance is cheaper dan nieuwe createn
        // we hebben maar 1, dus wordt nie gebruikt
        .base_pipeline_handle(vk::Pipeline::null())
        .base_pipeline_index(-1);

    data.pipeline = device.create_graphics_pipelines(
        vk::PipelineCache::null(),
        &[graphics_info],
        None
    )?.0[0];

    // modules zijn gwn simpele wrappper rond bytecode.
    // compilation & linking gebeurt pas eens de pipeline er is, dus dit is safe te deleten
    device.destroy_shader_module(vert_module, None);
    device.destroy_shader_module(frag_module, None);

    Ok(())
}

unsafe fn create_shader_module(
    device : &Device,
    bytecode : &[u8],
) -> Result<vk::ShaderModule> {
    let bytec = Bytecode::new(bytecode)?;
    let info = vk::ShaderModuleCreateInfo::builder()
        .code(bytec.code())
        .code_size(bytec.code_size());
    Ok(device.create_shader_module(&info, None)?)
}

unsafe fn create_render_pass(
    device : &Device,
    instance : &Instance,
    data : &mut AppData
) -> Result<()> {
    // NOTE pass beschrijft wat doen bij elke pass van rendering?
    let color_attachement = vk::AttachmentDescription::builder()
        // matched de swapchain, duh
        .format(data.swapchain_format)
        // geen multisampling, dus 1
        .samples(vk::SampleCountFlags::_1)
        // LOAD_OP & STORE_OP zijn voor color & depth data
        // before rendering, clear tot zwart voorda ge nieuwe frame maakt
        // LOAD: hou contents van attachment bij
        // CLEAR: clear ze voor een constant bij start
        // DONT_CARE: contents zijn undefine, don't care
        .load_op(vk::AttachmentLoadOp::CLEAR)
        // after rendering
        // STORE: contents bijhouden in memory 
        // DONT_CARE: contents van framebuffer zijn undefined na rendering
        .store_op(vk::AttachmentStoreOp::STORE)
        // stencil ops voor stencil data, tegenover color & depth van vorige 2
        // we doen niks met stencil buffer for now
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
        // gebruik zeker pixel format
        // init betekend da we niet caren wat de oute layout was
        .initial_layout(vk::ImageLayout::UNDEFINED)
        // final is welke layout naar transitionen eens render pass finishes
        // img moet klaar zijn voor presentation volgens de swapchain, dus PRES_SRC_KHR
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR);

    // ge kunt meerdere subpasses maken bvb voor een reeks postprocessing steps
    // dit kan voor optimizations zorgen.
    // Elke subpass hangt af van de vorige state
    let color_attachment_ref = vk::AttachmentReference::builder()
        // welke attachment index (we hebben er maar 1, dus index 0)
        // dit is *exact* de (location = 0) out vec4 outColor van fragShader
        .attachment(0)
        // vulkan transitioned naar de gewentste layout 
        // deze dient als color buffer, dus COLOR_ATTACHMENT_OPTIMAL is best gepast
        .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
    let color_attachments = &[color_attachment_ref];
    let subpass = vk::SubpassDescription::builder()
        //
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(color_attachments);

    let attachments = &[color_attachement];
    let subpasses = &[subpass];
    let info = vk::RenderPassCreateInfo::builder()
        .attachments(attachments)
        .subpasses(subpasses);

    data.render_pass = device.create_render_pass(&info, None)?;

    Ok(())
}




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
    data.swapchain_images = device.get_swapchain_images_khr(data.swapchain)?;
    data.swapchain_format = surface_format.format;
    data.swapchain_extent = extent;

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

unsafe fn create_swapchain_image_views(
    device : &Device,
    data: &mut AppData,
) -> Result<()> {
     let x = data
        .swapchain_images
        .iter()
        .map(|i| {

            let components = vk::ComponentMapping::builder()
                .r(vk::ComponentSwizzle::IDENTITY)
                .g(vk::ComponentSwizzle::IDENTITY)
                .b(vk::ComponentSwizzle::IDENTITY)
                .a(vk::ComponentSwizzle::IDENTITY);

            // layer count enzo op 1, verhogen als ge stereoscopic 3D wilt gaan
            let subresource_range = vk::ImageSubresourceRange::builder()
                .aspect_mask(vk::ImageAspectFlags::COLOR)
                .base_mip_level(0)
                .level_count(1)
                .base_array_layer(0)
                .layer_count(1);

            let info = vk::ImageViewCreateInfo::builder()
                .image(*i)
                .subresource_range(subresource_range)
                .view_type(vk::ImageViewType::_2D)
                .components(components)
                .format(data.swapchain_format);

            device.create_image_view(&info, None)
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(())
}

