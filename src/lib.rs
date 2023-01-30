use ash::{
    extensions::{
        ext::DebugUtils,
        khr::{Surface, Swapchain, VideoQueue},
    },
    vk::{native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN, KhrVideoQueueFn},
};

use ash::{vk, Entry};
pub use ash::{Device, Instance};

use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use anyhow::{anyhow, Result};

use winit;

use std::ffi::CStr;
use std::os::raw::c_char;
use std::{borrow::Cow, rc::Rc};

pub const DEBUG_ENABLED: bool = cfg!(debug_assertions);

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

pub struct App {
    entry: Entry,
    app_data: AppData,
    instance: Instance,
    device: Device,
}

impl App {
    pub unsafe fn create(window: &winit::window::Window) -> Result<Self> {
        let entry = Entry::linked();

        let mut app_data = AppData::default();

        let instance = create_instance(window, &entry, &mut app_data)?;

        app_data.surface = ash_window::create_surface(
            &entry,
            &instance,
            window.raw_display_handle(),
            window.raw_window_handle(),
            None,
        )?;

        let device = create_device(&instance, &entry, &mut app_data)?;

        Ok(Self {
            entry,
            app_data,
            instance,
            device,
        })
    }
    pub unsafe fn render(&mut self, window: &winit::window::Window) -> Result<()> {
        Ok(())
    }
    pub unsafe fn destroy(&mut self) {
        self.device.device_wait_idle().unwrap();

        let surface_loader = Surface::new(&self.entry, &self.instance);
        surface_loader.destroy_surface(self.app_data.surface, None);

        let debug_utils_loader = DebugUtils::new(&self.entry, &self.instance);
        debug_utils_loader.destroy_debug_utils_messenger(self.app_data.debug_call_back, None);

        self.device.destroy_device(None);
        self.instance.destroy_instance(None);
    }
}

#[derive(Clone, Debug, Default)]
pub struct AppData {
    pub debug_call_back: vk::DebugUtilsMessengerEXT,
    pub surface: vk::SurfaceKHR,
    pub physical_device: vk::PhysicalDevice,
    pub grapgics_queue_family_index: u32,
    pub decode_queue_family_index: u32,
    pub graphics_queue: vk::Queue,
}

pub unsafe fn create_instance(
    window: &winit::window::Window,
    entry: &Entry,
    app_data: &mut AppData,
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

        app_data.debug_call_back =
            debug_utils_loader.create_debug_utils_messenger(&debug_info, None)?
    }

    Ok(instance)
}

pub unsafe fn create_device(
    instance: &Instance,
    entry: &Entry,
    app_data: &mut AppData,
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
                    app_data.decode_queue_family_index = k as u32;
                }
            }

            if queue_family_property
                .queue_family_properties
                .queue_flags
                .contains(vk::QueueFlags::GRAPHICS)
                && surface_loader
                    .get_physical_device_surface_support(pdevice, k as u32, app_data.surface)
                    .unwrap()
            {
                found_graphics_queue = true;
                app_data.grapgics_queue_family_index = k as u32;
            }
        }

        if found_decode_queue && found_graphics_queue {
            app_data.physical_device = pdevice;
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
        app_data.decode_queue_family_index
    );
    println!(
        "Graphics queue family index: {:?}",
        app_data.grapgics_queue_family_index
    );

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

    video_queue_loader.get_physical_device_video_capabilities_khr(
        app_data.physical_device,
        &profile_info,
        &mut capabilities,
    )?;

    let profiles = vec![profile_info];

    let mut profile_list_info = vk::VideoProfileListInfoKHR::default().profiles(&profiles);

    let format_info =
        vk::PhysicalDeviceVideoFormatInfoKHR::default().push_next(&mut profile_list_info).image_usage(
            vk::ImageUsageFlags::VIDEO_DECODE_SRC_KHR
        );

    let format_properties_count = video_queue_loader
        .get_physical_device_video_format_properties_khr_len(
            app_data.physical_device,
            &format_info,
        );

    let mut format_properties =
        vec![vk::VideoFormatPropertiesKHR::default(); format_properties_count];

    video_queue_loader.get_physical_device_video_format_properties_khr(app_data.physical_device, &format_info, &mut format_properties)?;

    let device_extension_names_raw = [Swapchain::name().as_ptr(), KhrVideoQueueFn::name().as_ptr()];
    let features = vk::PhysicalDeviceFeatures {
        shader_clip_distance: 1,
        ..Default::default()
    };
    let priorities = [0.0];

    let graphics_queue_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(app_data.grapgics_queue_family_index)
        .queue_priorities(&priorities);

    let decode_queue_info = vk::DeviceQueueCreateInfo::default()
        .queue_family_index(app_data.decode_queue_family_index)
        .queue_priorities(&priorities);

    let queue_infos = [graphics_queue_info, decode_queue_info];

    let device_create_info = vk::DeviceCreateInfo::default()
        .queue_create_infos(&queue_infos)
        .enabled_extension_names(&device_extension_names_raw)
        .enabled_features(&features);

    let device: Device = instance
        .create_device(app_data.physical_device, &device_create_info, None)
        .unwrap();

    Ok(device)
}
