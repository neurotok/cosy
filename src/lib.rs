use ash::{
    extensions::{
        ext::DebugUtils,
        khr::{Surface, Swapchain, VideoQueue},
    },
    vk::{
        native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN, KhrVideoQueueFn,
        SwapchainKHR,
    },
};

use ash::{vk, Entry};
pub use ash::{Device, Instance};

use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use anyhow::{anyhow, Result};

use winit;

use std::borrow::Cow;
use std::ffi::CStr;
use std::mem::{self, align_of};
use std::os::raw::c_char;

pub const DEBUG_ENABLED: bool = cfg!(debug_assertions);

#[macro_export]
macro_rules! offset_of {
    ($base:path, $field:ident) => {{
        #[allow(unused_unsafe)]
        unsafe {
            let b: $base = mem::zeroed();
            std::ptr::addr_of!(b.$field) as isize - std::ptr::addr_of!(b) as isize
        }
    }};
}

#[allow(clippy::too_many_arguments)]
pub fn record_submit_commandbuffer<F: FnOnce(&Device, vk::CommandBuffer)>(
    device: &Device,
    command_buffer: vk::CommandBuffer,
    command_buffer_reuse_fence: vk::Fence,
    submit_queue: vk::Queue,
    wait_mask: &[vk::PipelineStageFlags],
    wait_semaphores: &[vk::Semaphore],
    signal_semaphores: &[vk::Semaphore],
    f: F,
) {
    unsafe {
        device
            .wait_for_fences(&[command_buffer_reuse_fence], true, std::u64::MAX)
            .expect("Wait for fence failed.");

        device
            .reset_fences(&[command_buffer_reuse_fence])
            .expect("Reset fences failed.");

        device
            .reset_command_buffer(
                command_buffer,
                vk::CommandBufferResetFlags::RELEASE_RESOURCES,
            )
            .expect("Reset command buffer failed.");

        let command_buffer_begin_info = vk::CommandBufferBeginInfo::default()
            .flags(vk::CommandBufferUsageFlags::ONE_TIME_SUBMIT);

        device
            .begin_command_buffer(command_buffer, &command_buffer_begin_info)
            .expect("Begin commandbuffer");
        f(device, command_buffer);
        device
            .end_command_buffer(command_buffer)
            .expect("End commandbuffer");

        let command_buffers = vec![command_buffer];

        let submit_info = vk::SubmitInfo::default()
            .wait_semaphores(wait_semaphores)
            .wait_dst_stage_mask(wait_mask)
            .command_buffers(&command_buffers)
            .signal_semaphores(signal_semaphores);

        device
            .queue_submit(submit_queue, &[submit_info], command_buffer_reuse_fence)
            .expect("queue submit failed.");
    }
}

unsafe extern "system" fn vulkan_debug_callback(
    message_severity: vk::DebugUtilsMessageSeverityFlagsEXT,
    message_type: vk::DebugUtilsMessageTypeFlagsEXT,
    p_callback_data: *const vk::DebugUtilsMessengerCallbackDataEXT,
    _user_data: *mut std::os::raw::c_void,
) -> vk::Bool32 {
    let callback_data = *p_callback_data;
    let message_id_number = callback_data.message_id_number;

    let message_id_name = if callback_data.p_message_id_name.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message_id_name).to_string_lossy()
    };

    let message = if callback_data.p_message.is_null() {
        Cow::from("")
    } else {
        CStr::from_ptr(callback_data.p_message).to_string_lossy()
    };

    println!(
        "{:?}:\n{:?} [{} ({})] : {}\n",
        message_severity, message_type, message_id_name, message_id_number, message,
    );

    vk::FALSE
}

pub fn find_memorytype_index(
    memory_req: &vk::MemoryRequirements,
    memory_prop: &vk::PhysicalDeviceMemoryProperties,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    memory_prop.memory_types[..memory_prop.memory_type_count as _]
        .iter()
        .enumerate()
        .find(|(index, memory_type)| {
            (1 << index) & memory_req.memory_type_bits != 0
                && memory_type.property_flags & flags == flags
        })
        .map(|(index, _memory_type)| index as _)
}

pub struct App {
    pub entry: Entry,
    pub data: AppData,
    pub instance: Instance,
    pub device: Device,
}

impl App {
    pub unsafe fn create(
        window: &winit::window::Window,
        window_width: u32,
        window_height: u32,
    ) -> Result<Self> {
        let entry = Entry::linked();

        let mut data = AppData::new(window_width, window_height);

        let instance = create_instance(window, &entry, &mut data)?;

        data.surface = ash_window::create_surface(
            &entry,
            &instance,
            window.raw_display_handle(),
            window.raw_window_handle(),
            None,
        )?;

        let device = create_device(&instance, &entry, &mut data)?;

        data.swapchain = create_swapchain(&instance, &device, &entry, &mut data)?;
        data.swapchain_image_views = create_swapchain_image_views(&instance, &device, &mut data)?;

        data.device_memory_properties =
            instance.get_physical_device_memory_properties(data.physical_device);

        let fence_create_info =
            vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

        data.setup_commands_reuse_fence = device
            .create_fence(&fence_create_info, None)
            .expect("Create fence failed.");

        data.depth_image_view = create_depth_image_view(&instance, &device, &mut data)?;

        let semaphore_create_info = vk::SemaphoreCreateInfo::default();

        let present_complete_semaphore = device
            .create_semaphore(&semaphore_create_info, None)
            .unwrap();
        let rendering_complete_semaphore = device
            .create_semaphore(&semaphore_create_info, None)
            .unwrap();

        Ok(Self {
            entry,
            data,
            instance,
            device,
        })
    }
    pub unsafe fn render(&mut self, window: &winit::window::Window) -> Result<()> {
        let (present_index, _) = base
            .swapchain_loader
            .acquire_next_image(
                base.swapchain,
                std::u64::MAX,
                base.present_complete_semaphore,
                vk::Fence::null(),
            )
            .unwrap();
        let clear_values = [
            vk::ClearValue {
                color: vk::ClearColorValue {
                    float32: [0.0, 0.0, 0.0, 0.0],
                },
            },
            vk::ClearValue {
                depth_stencil: vk::ClearDepthStencilValue {
                    depth: 1.0,
                    stencil: 0,
                },
            },
        ];

        let render_pass_begin_info = vk::RenderPassBeginInfo::default()
            .render_pass(renderpass)
            .framebuffer(framebuffers[present_index as usize])
            .render_area(base.surface_resolution.into())
            .clear_values(&clear_values);

        record_submit_commandbuffer(
            &base.device,
            base.draw_command_buffer,
            base.draw_commands_reuse_fence,
            base.present_queue,
            &[vk::PipelineStageFlags::BOTTOM_OF_PIPE],
            &[base.present_complete_semaphore],
            &[base.rendering_complete_semaphore],
            |device, draw_command_buffer| {
                device.cmd_begin_render_pass(
                    draw_command_buffer,
                    &render_pass_begin_info,
                    vk::SubpassContents::INLINE,
                );
                device.cmd_bind_descriptor_sets(
                    draw_command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    pipeline_layout,
                    0,
                    &descriptor_sets[..],
                    &[],
                );
                device.cmd_bind_pipeline(
                    draw_command_buffer,
                    vk::PipelineBindPoint::GRAPHICS,
                    graphic_pipeline,
                );
                device.cmd_set_viewport(draw_command_buffer, 0, &viewports);
                device.cmd_set_scissor(draw_command_buffer, 0, &scissors);
                device.cmd_bind_vertex_buffers(
                    draw_command_buffer,
                    0,
                    &[vertex_input_buffer],
                    &[0],
                );
                device.cmd_bind_index_buffer(
                    draw_command_buffer,
                    index_buffer,
                    0,
                    vk::IndexType::UINT32,
                );
                device.cmd_draw_indexed(
                    draw_command_buffer,
                    index_buffer_data.len() as u32,
                    1,
                    0,
                    0,
                    1,
                );
                // Or draw without the index buffer
                // device.cmd_draw(draw_command_buffer, 3, 1, 0, 0);
                device.cmd_end_render_pass(draw_command_buffer);
            },
        );
        //let mut present_info_err = mem::zeroed();
        let present_info = vk::PresentInfoKHR {
            wait_semaphore_count: 1,
            p_wait_semaphores: &base.rendering_complete_semaphore,
            swapchain_count: 1,
            p_swapchains: &base.swapchain,
            p_image_indices: &present_index,
            ..Default::default()
        };
        base.swapchain_loader
            .queue_present(base.present_queue, &present_info)
            .unwrap();

        Ok(())
    }
    pub unsafe fn destroy(&mut self) {
        self.device.device_wait_idle().unwrap();

        let swapchain_loader = Swapchain::new(&self.instance, &self.device);
        swapchain_loader.destroy_swapchain(self.data.swapchain, None);

        let surface_loader = Surface::new(&self.entry, &self.instance);
        surface_loader.destroy_surface(self.data.surface, None);

        let debug_utils_loader = DebugUtils::new(&self.entry, &self.instance);
        debug_utils_loader.destroy_debug_utils_messenger(self.data.debug_call_back, None);

        self.device.destroy_device(None);
        self.instance.destroy_instance(None);
    }
}

// TODO general image
#[derive(Clone, Debug, Default)]
pub struct Image {
    pub image_memory: vk::DeviceMemory,
    pub image_view: vk::ImageView,
}

#[derive(Clone, Debug, Default)]
pub struct AppData {
    pub debug_call_back: vk::DebugUtilsMessengerEXT,
    pub surface: vk::SurfaceKHR,
    pub physical_device: vk::PhysicalDevice,
    pub graphics_queue_family_index: u32,
    pub decode_queue_family_index: u32,
    pub graphics_queue: vk::Queue,
    pub present_queue: vk::Queue,
    pub window_width: u32,
    pub window_height: u32,
    pub swapchain: SwapchainKHR,
    pub surface_format: vk::SurfaceFormatKHR,
    pub surface_resolution: vk::Extent2D,
    pub swapchain_image_views: Vec<vk::ImageView>,
    pub setup_command_buffer: vk::CommandBuffer,
    pub draw_command_buffer: vk::CommandBuffer,
    pub device_memory_properties: vk::PhysicalDeviceMemoryProperties,
    pub setup_commands_reuse_fence: vk::Fence,
    pub depth_image_view: vk::ImageView,
    //pub depth_image: Image,
}

impl AppData {
    fn new(window_width: u32, window_height: u32) -> Self {
        Self {
            window_width,
            window_height,
            ..Default::default()
        }
    }
}

pub unsafe fn create_instance(
    window: &winit::window::Window,
    entry: &Entry,
    data: &mut AppData,
) -> Result<Instance> {
    let app_name = CStr::from_bytes_with_nul_unchecked(b"VulkanTriangle\0");

    let layer_names = [CStr::from_bytes_with_nul_unchecked(
        b"VK_LAYER_KHRONOS_validation\0",
    )];

    let layers_names_raw: Vec<*const c_char> = if DEBUG_ENABLED {
        layer_names
            .iter()
            .map(|raw_name| raw_name.as_ptr())
            .collect()
    } else {
        Vec::new()
    };

    let mut extension_names =
        ash_window::enumerate_required_extensions(window.raw_display_handle())
            .unwrap()
            .to_vec();

    if DEBUG_ENABLED {
        extension_names.push(DebugUtils::name().as_ptr());
    }

    let appinfo = vk::ApplicationInfo::default()
        .application_name(app_name)
        .application_version(0)
        .engine_name(app_name)
        .engine_version(0)
        .api_version(vk::make_api_version(0, 1, 3, 0));

    let create_flags = vk::InstanceCreateFlags::default();

    let create_info = vk::InstanceCreateInfo::default()
        .application_info(&appinfo)
        .enabled_extension_names(&extension_names)
        .enabled_layer_names(&layers_names_raw)
        .flags(create_flags);

    let instance: Instance = entry
        .create_instance(&create_info, None)
        .expect("Instance creation error");

    if DEBUG_ENABLED {
        let debug_info = vk::DebugUtilsMessengerCreateInfoEXT::default()
            .message_severity(
                vk::DebugUtilsMessageSeverityFlagsEXT::ERROR
                    | vk::DebugUtilsMessageSeverityFlagsEXT::WARNING
                    | vk::DebugUtilsMessageSeverityFlagsEXT::INFO,
            )
            .message_type(
                vk::DebugUtilsMessageTypeFlagsEXT::GENERAL
                    | vk::DebugUtilsMessageTypeFlagsEXT::VALIDATION
                    | vk::DebugUtilsMessageTypeFlagsEXT::PERFORMANCE,
            )
            .pfn_user_callback(Some(vulkan_debug_callback));

        let debug_utils_loader = DebugUtils::new(&entry, &instance);

        data.debug_call_back = debug_utils_loader.create_debug_utils_messenger(&debug_info, None)?
    }

    Ok(instance)
}

pub unsafe fn create_device(
    instance: &Instance,
    entry: &Entry,
    data: &mut AppData,
) -> Result<Device> {
    let pdevices = instance
        .enumerate_physical_devices()
        .expect("Physical device error");

    let surface_loader = Surface::new(&entry, &instance);

    let mut found_graphics_queue = false;
    let mut found_decode_queue = false;

    for i in 0..pdevices.len() {
        let pdevice = pdevices[i];

        found_graphics_queue = false;
        found_decode_queue = false;

        let queue_family_properties_count =
            instance.get_physical_device_queue_family_properties2_len(pdevice);

        let mut video_queue_family_properties =
            vec![vk::QueueFamilyVideoPropertiesKHR::default(); queue_family_properties_count];
        let mut queue_family_properties =
            vec![vk::QueueFamilyProperties2::default(); queue_family_properties_count];

        for j in 0..queue_family_properties_count {
            //push_next only implemented for struct builders
            queue_family_properties[j].p_next =
                &mut video_queue_family_properties[j] as *mut _ as _;
        }

        instance
            .get_physical_device_queue_family_properties2(pdevice, &mut queue_family_properties);

        for k in 0..queue_family_properties.len() {
            let queue_family_property = queue_family_properties[k];
            let video_queue_family_property = video_queue_family_properties[k];

            if queue_family_property
                .queue_family_properties
                .queue_flags
                .contains(vk::QueueFlags::VIDEO_DECODE_KHR)
            {
                if video_queue_family_property
                    .video_codec_operations
                    .contains(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
                {
                    found_decode_queue = true;
                    data.decode_queue_family_index = k as u32;
                }
            }

            if queue_family_property
                .queue_family_properties
                .queue_flags
                .contains(vk::QueueFlags::GRAPHICS)
                && surface_loader
                    .get_physical_device_surface_support(pdevice, k as u32, data.surface)
                    .unwrap()
            {
                found_graphics_queue = true;
                data.graphics_queue_family_index = k as u32;
            }
        }

        if found_decode_queue && found_graphics_queue {
            data.physical_device = pdevice;
            break;
        }
    }

    if !found_decode_queue {
        return Err(anyhow!(
            "H264 video decode is not supported on this platform"
        ));
    }
    if !found_graphics_queue {
        return Err(anyhow!(
            "Graphics display is not supported on this platform"
        ));
    }

    println!(
        "Decode queue family index: {:?}",
        data.decode_queue_family_index
    );
    println!(
        "Graphics queue family index: {:?}",
        data.graphics_queue_family_index
    );

    let device_extension_names_raw = [Swapchain::name().as_ptr(), KhrVideoQueueFn::name().as_ptr()];
    let features = vk::PhysicalDeviceFeatures {
        shader_clip_distance: 1,
        ..Default::default()
    };
    let priorities = [0.0];

    let graphics_queue_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(data.graphics_queue_family_index)
        .queue_priorities(&priorities);

    let decode_queue_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(data.decode_queue_family_index)
        .queue_priorities(&priorities);

    let queue_infos = [graphics_queue_info, decode_queue_info];

    let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&device_extension_names_raw)
        .enabled_features(&features);

    let device: Device = instance
        .create_device(data.physical_device, &device_create_info, None)
        .unwrap();

    Ok(device)
}

pub unsafe fn create_swapchain(
    instance: &Instance,
    device: &Device,
    entry: &Entry,
    data: &mut AppData,
) -> Result<SwapchainKHR> {
    data.present_queue = device.get_device_queue(data.graphics_queue_family_index, 0);

    let surface_loader = Surface::new(&entry, &instance);

    data.surface_format = surface_loader
        .get_physical_device_surface_formats(data.physical_device, data.surface)
        .unwrap()[0];

    let surface_capabilities = surface_loader
        .get_physical_device_surface_capabilities(data.physical_device, data.surface)
        .unwrap();

    let mut desired_image_count = surface_capabilities.min_image_count + 1;
    if surface_capabilities.max_image_count > 0
        && desired_image_count > surface_capabilities.max_image_count
    {
        desired_image_count = surface_capabilities.max_image_count;
    }

    data.surface_resolution = match surface_capabilities.current_extent.width {
        std::u32::MAX => vk::Extent2D {
            width: data.window_width,
            height: data.window_height,
        },
        _ => surface_capabilities.current_extent,
    };
    let pre_transform = if surface_capabilities
        .supported_transforms
        .contains(vk::SurfaceTransformFlagsKHR::IDENTITY)
    {
        vk::SurfaceTransformFlagsKHR::IDENTITY
    } else {
        surface_capabilities.current_transform
    };
    let present_modes = surface_loader
        .get_physical_device_surface_present_modes(data.physical_device, data.surface)?;

    let present_mode = present_modes
        .iter()
        .cloned()
        .find(|&mode| mode == vk::PresentModeKHR::MAILBOX)
        .unwrap_or(vk::PresentModeKHR::FIFO);

    let swapchain_create_info = vk::SwapchainCreateInfoKHR::default()
        .surface(data.surface)
        .min_image_count(desired_image_count)
        .image_color_space(data.surface_format.color_space)
        .image_format(data.surface_format.format)
        .image_extent(data.surface_resolution)
        .image_usage(vk::ImageUsageFlags::COLOR_ATTACHMENT)
        .image_sharing_mode(vk::SharingMode::EXCLUSIVE)
        .pre_transform(pre_transform)
        .composite_alpha(vk::CompositeAlphaFlagsKHR::OPAQUE)
        .present_mode(present_mode)
        .clipped(true)
        .image_array_layers(1);

    let swapchain_loader = Swapchain::new(&instance, &device);

    let swapchain = swapchain_loader.create_swapchain(&swapchain_create_info, None)?;

    Ok(swapchain)
}

pub unsafe fn create_swapchain_image_views(
    instance: &Instance,
    device: &Device,
    data: &mut AppData,
) -> Result<Vec<vk::ImageView>> {
    let pool_create_info = vk::CommandPoolCreateInfo::default()
        .flags(vk::CommandPoolCreateFlags::RESET_COMMAND_BUFFER)
        .queue_family_index(data.graphics_queue_family_index);

    let pool = device.create_command_pool(&pool_create_info, None).unwrap();

    let command_buffer_allocate_info = vk::CommandBufferAllocateInfo::default()
        .command_buffer_count(2)
        .command_pool(pool)
        .level(vk::CommandBufferLevel::PRIMARY);

    let command_buffers = device
        .allocate_command_buffers(&command_buffer_allocate_info)
        .unwrap();
    data.setup_command_buffer = command_buffers[0];
    data.draw_command_buffer = command_buffers[1];

    let swapchain_loader = Swapchain::new(&instance, &device);

    let present_images = swapchain_loader
        .get_swapchain_images(data.swapchain)
        .unwrap();
    let present_image_views: Vec<vk::ImageView> = present_images
        .iter()
        .map(|&image| {
            let create_view_info = vk::ImageViewCreateInfo::default()
                .view_type(vk::ImageViewType::TYPE_2D)
                .format(data.surface_format.format)
                .components(vk::ComponentMapping {
                    r: vk::ComponentSwizzle::R,
                    g: vk::ComponentSwizzle::G,
                    b: vk::ComponentSwizzle::B,
                    a: vk::ComponentSwizzle::A,
                })
                .subresource_range(vk::ImageSubresourceRange {
                    aspect_mask: vk::ImageAspectFlags::COLOR,
                    base_mip_level: 0,
                    level_count: 1,
                    base_array_layer: 0,
                    layer_count: 1,
                })
                .image(image);
            device.create_image_view(&create_view_info, None).unwrap()
        })
        .collect();
    Ok(present_image_views)
}

pub unsafe fn create_depth_image_view(
    instance: &Instance,
    device: &Device,
    data: &mut AppData,
) -> Result<vk::ImageView> {
    let depth_image_create_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(vk::Format::D16_UNORM)
        .extent(data.surface_resolution.into())
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::DEPTH_STENCIL_ATTACHMENT)
        .sharing_mode(vk::SharingMode::EXCLUSIVE);

    let depth_image = device.create_image(&depth_image_create_info, None).unwrap();
    let depth_image_memory_req = device.get_image_memory_requirements(depth_image);
    let depth_image_memory_index = find_memorytype_index(
        &depth_image_memory_req,
        &data.device_memory_properties,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )
    .expect("Unable to find suitable memory index for depth image.");

    let depth_image_allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(depth_image_memory_req.size)
        .memory_type_index(depth_image_memory_index);

    let depth_image_memory = device
        .allocate_memory(&depth_image_allocate_info, None)
        .unwrap();

    device
        .bind_image_memory(depth_image, depth_image_memory, 0)
        .expect("Unable to bind depth image memory");

    let fence_create_info = vk::FenceCreateInfo::default().flags(vk::FenceCreateFlags::SIGNALED);

    let draw_commands_reuse_fence = device
        .create_fence(&fence_create_info, None)
        .expect("Create fence failed.");

    record_submit_commandbuffer(
        &device,
        data.setup_command_buffer,
        data.setup_commands_reuse_fence,
        data.present_queue,
        &[],
        &[],
        &[],
        |device, setup_command_buffer| {
            let layout_transition_barriers = vk::ImageMemoryBarrier::default()
                .image(depth_image)
                .dst_access_mask(
                    vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_READ
                        | vk::AccessFlags::DEPTH_STENCIL_ATTACHMENT_WRITE,
                )
                .new_layout(vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL)
                .old_layout(vk::ImageLayout::UNDEFINED)
                .subresource_range(
                    vk::ImageSubresourceRange::default()
                        .aspect_mask(vk::ImageAspectFlags::DEPTH)
                        .layer_count(1)
                        .level_count(1),
                );

            device.cmd_pipeline_barrier(
                setup_command_buffer,
                vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                vk::PipelineStageFlags::LATE_FRAGMENT_TESTS,
                vk::DependencyFlags::empty(),
                &[],
                &[],
                &[layout_transition_barriers],
            );
        },
    );

    let depth_image_view_info = vk::ImageViewCreateInfo::default()
        .subresource_range(
            vk::ImageSubresourceRange::default()
                .aspect_mask(vk::ImageAspectFlags::DEPTH)
                .level_count(1)
                .layer_count(1),
        )
        .image(depth_image)
        .format(depth_image_create_info.format)
        .view_type(vk::ImageViewType::TYPE_2D);

    let depth_image_view = device
        .create_image_view(&depth_image_view_info, None)
        .unwrap();

    Ok(depth_image_view)
}

pub unsafe fn create_h264_video_decode_profile_list(
    instance: &Instance,
    entry: &Entry,
    data: &mut AppData,
) {
    let mut video_profile_operation = vk::VideoDecodeH264ProfileInfoKHR::default()
        .std_profile_idc(StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN)
        .picture_layout(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE);

    let profile_info = vk::VideoProfileInfoKHR::default()
        .push_next(&mut video_profile_operation)
        .video_codec_operation(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
        .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
        .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
        .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8);

    let mut h264_decode_capibilities = vk::VideoDecodeH264CapabilitiesKHR::default();

    let mut decode_capabilities = vk::VideoDecodeCapabilitiesKHR::default();
    // TODO no p_next or push_next motheods yet this is failing when not passed
    decode_capabilities.p_next = &mut h264_decode_capibilities as *mut _ as _;

    let mut capabilities = vk::VideoCapabilitiesKHR::default().push_next(&mut decode_capabilities);

    let video_queue_loader = VideoQueue::new(entry, instance);

    video_queue_loader
        .get_physical_device_video_capabilities_khr(
            data.physical_device,
            &profile_info,
            &mut capabilities,
        )
        .unwrap();
    //)?;

    let profiles = vec![profile_info];

    let mut profile_list_info = vk::VideoProfileListInfoKHR::default().profiles(&profiles);

    let format_info = vk::PhysicalDeviceVideoFormatInfoKHR::default()
        .push_next(&mut profile_list_info)
        .image_usage(vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR);

    let format_properties_count = video_queue_loader
        .get_physical_device_video_format_properties_khr_len(data.physical_device, &format_info);

    let mut format_properties =
        vec![vk::VideoFormatPropertiesKHR::default(); format_properties_count];

    video_queue_loader
        .get_physical_device_video_format_properties_khr(
            data.physical_device,
            &format_info,
            &mut format_properties, //)?;
        )
        .unwrap();
}
