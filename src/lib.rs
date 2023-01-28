use ash::{
    extensions::{
        ext::DebugUtils,
        khr::{Surface, Swapchain, VideoQueue},
    },
    vk::KhrVideoQueueFn,
};

use ash::{vk, Entry};
pub use ash::{Device, Instance};

use raw_window_handle::{HasRawDisplayHandle, HasRawWindowHandle};

use anyhow::{anyhow, Result};

use winit;

use std::{borrow::Cow, rc::Rc};
use std::ffi::CStr;
use std::os::raw::c_char;

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

impl Drop for App {
    fn drop(&mut self) {
        unsafe {
            self.device.device_wait_idle().unwrap();
            self.device.destroy_device(None);
            self.instance.destroy_instance(None);
        }
    }
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

        let mut video_queue_family_properties = vk::QueueFamilyVideoPropertiesKHR::default();
        
        let queue_family_properties_count = instance.get_physical_device_queue_family_properties2_len(pdevice);

        assert_eq!(queue_family_properties_count, 1);

            let mut queue_family_properties = vec![vk::QueueFamilyProperties2::default()
                .push_next(&mut video_queue_family_properties)];

        instance
            .get_physical_device_queue_family_properties2(pdevice, &mut queue_family_properties);

        for j in 0..queue_family_properties.len() {
            let queue_family_property = queue_family_properties[j];

            if queue_family_property
                .queue_family_properties
                .queue_flags
                .contains(vk::QueueFlags::VIDEO_DECODE_KHR)
            {
                if video_queue_family_properties
                    .video_codec_operations
                    .contains(vk::VideoCodecOperationFlagsKHR::DECODE_H264)
                    // Nvidia driver bug
                    || video_queue_family_properties
                    .video_codec_operations
                    .contains(std::mem::transmute::<u32, vk::VideoCodecOperationFlagsKHR>(0x10000))
                {
                    found_decode_queue = true;
                    app_data.decode_queue_family_index = j as u32;
                }
            }

            if queue_family_property
                .queue_family_properties
                .queue_flags
                .contains(vk::QueueFlags::GRAPHICS)
                && surface_loader
                    .get_physical_device_surface_support(pdevice, j as u32, app_data.surface)
                    .unwrap()
            {
                found_graphics_queue = true;
                app_data.grapgics_queue_family_index = j as u32;
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


    let mut profile_usage_info = vk::VideoDecodeUsageInfoKHR::default();
    // .video_usage_hints( vk::VideoDecodeUsageFlagsKHR::DEFAULT)
    // .build();

    let profile_info = vk::VideoProfileInfoKHR::default()
    .push_next(&mut profile_usage_info)
    .chroma_subsampling(vk::VideoChromaSubsamplingFlagsKHR::TYPE_420)
    .luma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8)
    .chroma_bit_depth(vk::VideoComponentBitDepthFlagsKHR::TYPE_8);

    let mut decode_capapitilied = vk::VideoDecodeCapabilitiesKHR::default();

    let capabilities = vk::VideoCapabilitiesKHR::default()
    .push_next(&mut decode_capapitilied);


    //  let handle = instance.handle();

    // let video_queue_loader = KhrVideoQueueFn::load(|name| {
    //     std::mem::transmute(entry.get_instance_proc_addr(handle, name.as_ptr()))
    // });




    


	//video_queue_fps.get_physical_device_video_capabilities_khr(pdevices, &mut profile_info, &mut capabilities);  



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
