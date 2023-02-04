use std::default::Default;
use std::env;
use std::ffi::CStr;
use std::io::{Cursor, Read};
use std::mem::{self, align_of};
use std::os::raw::c_void;

use ash::extensions::khr::{VideoQueue, VideoDecodeQueue};
use ash::util::*;
use ash::vk;
use ash::vk::{KhrVideoQueueFn, KhrVideoDecodeQueueFn};
use ash::vk::native::StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN;

use anyhow::Result;

use cozy::*;

use mp4parse;

struct VideoSpec {
    width: u16,
    height: u16,
    codec_type: mp4parse::CodecType,
}

impl Default for VideoSpec {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            codec_type: mp4parse::CodecType::Unknown,
        }
    }
}

#[derive(Clone, Debug, Copy)]
struct Vertex {
    pos: [f32; 4],
    uv: [f32; 2],
}

#[derive(Clone, Debug, Copy)]
pub struct Vector3 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub _pad: f32,
}

fn main() -> Result<()> {
    unsafe {
        let args: Vec<String> = env::args().collect();

        let mut file = std::fs::File::open({
            if DEBUG_ENABLED {
                "./samples/Big_Buck_Bunny_360_10s_1MB.mp4"
            } else {
                &args[1]
            }
        })?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;

        let mut c = Cursor::new(&buf);
        let video_context = mp4parse::read_mp4(&mut c)?;

        let mut video_spec = VideoSpec::default();

        assert_eq!(
            video_context.timescale,
            Some(mp4parse::MediaTimeScale(1000))
        );

        for track in video_context.tracks {
            match track.track_type {
                mp4parse::TrackType::Video => {
                    let stsd = track.stsd.expect("expected an stsd");

                    let stcs = track.stsc.expect("expected an stsc");

                    let v = match stsd.descriptions.first().expect("expected a SampleEntry") {
                        mp4parse::SampleEntry::Video(v) => v,
                        _ => panic!("expected a VideoSampleEntry"),
                    };

                    if DEBUG_ENABLED {
                        assert_eq!(v.width, 640);
                        assert_eq!(v.height, 360);
                        assert_eq!(v.codec_type, mp4parse::CodecType::H264);
                    }

                    video_spec.width = v.width;
                    video_spec.height = v.height;
                    video_spec.codec_type = v.codec_type;

                    assert_eq!(
                        match v.codec_specific {
                            mp4parse::VideoCodecSpecific::AVCConfig(ref avc) => {
                                assert!(!avc.is_empty());
                                "AVC"
                            }
                            mp4parse::VideoCodecSpecific::VPxConfig(ref vpx) => {
                                // We don't enter in here, we just check if fields are public.
                                assert!(vpx.bit_depth > 0);
                                assert!(vpx.colour_primaries > 0);
                                assert!(vpx.chroma_subsampling > 0);
                                assert!(!vpx.codec_init.is_empty());
                                "VPx"
                            }
                            mp4parse::VideoCodecSpecific::ESDSConfig(ref mp4v) => {
                                assert!(!mp4v.is_empty());
                                "MP4V"
                            }
                            mp4parse::VideoCodecSpecific::AV1Config(ref _av1c) => {
                                "AV1"
                            }
                            mp4parse::VideoCodecSpecific::H263Config(ref _h263) => {
                                "H263"
                            }
                        },
                        "AVC"
                    );
                }
                _ => {}
            }
        }

        let base = ExampleBase::new(video_spec.width as u32, video_spec.height as u32)?;

        let mut video_decode_usage_info = vk::VideoDecodeUsageInfoKHR::default()
            .video_usage_hints(vk::VideoDecodeUsageFlagsKHR::OFFLINE);

        let mut video_profile_operation = vk::VideoDecodeH264ProfileInfoKHR::default()
            .std_profile_idc(StdVideoH264ProfileIdc_STD_VIDEO_H264_PROFILE_IDC_MAIN)
            .picture_layout(vk::VideoDecodeH264PictureLayoutFlagsKHR::PROGRESSIVE);

        video_profile_operation.p_next = &mut video_decode_usage_info as *mut _ as _;

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

        let mut capabilities =
            vk::VideoCapabilitiesKHR::default().push_next(&mut decode_capabilities);

        //let video_queue_loader = VideoQueue::new(&base.instance, &base.device);
        let video_queue_loader = VideoQueue::new(&base.entry, &base.instance);

        let video_decode_queue_loader = VideoDecodeQueue::new(&base.instance, &base.device);

        video_queue_loader
            .get_physical_device_video_capabilities_khr(
                base.pdevice,
                &profile_info,
                &mut capabilities,
            )
            .unwrap();

        /*
        let video_profiles = vec![profile_info];

        let mut profile_list_info =
            vk::VideoProfileListInfoKHR::default().profiles(&video_profiles);

        let mut dst_video_format = find_video_format(
            base.pdevice,
            &video_queue_loader,
            vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR,
            &mut profile_list_info,
        )?;
        let mut dpb_video_format = find_video_format(
            base.pdevice,
            &video_queue_loader,
            vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR,
            &mut profile_list_info,
        )?;

        //TODO experimental
        let prefer_coincide_mode = false;

        if decode_capabilities
            .flags
            .contains(vk::VideoDecodeCapabilityFlagsKHR::DPB_AND_OUTPUT_DISTINCT)
            && prefer_coincide_mode
        {
            dst_video_format = find_video_format(
                base.pdevice,
                &video_queue_loader,
                vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR
                    | vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR,
                &mut profile_list_info,
            )?;

            dpb_video_format = dst_video_format;
        }

        let bitstream_buffer_info = vk::BufferCreateInfo {
            p_next: &mut profile_list_info as *mut _ as _,
            size: buf.len() as u64,
            usage: vk::BufferUsageFlags::VIDEO_DECODE_DST_KHR,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        };

        let bistream_buffer = base
            .device
            .create_buffer(&bitstream_buffer_info, None)
            .unwrap();

        let video_extent = vk::Extent2D {
            width: video_spec.width as u32,
            height: video_spec.height as u32,
        };
    
        // DST

        let dst_image_create_info = vk::ImageCreateInfo {
            p_next: &mut profile_list_info as *mut _ as _,
            image_type: vk::ImageType::TYPE_2D,
            format: dst_video_format,
            extent: video_extent.into(),
            mip_levels: 1,
            array_layers: 1,
            samples: vk::SampleCountFlags::TYPE_1,
            tiling: vk::ImageTiling::OPTIMAL,
            usage: vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR,
            ..Default::default()
        };

        let dst_image = base
            .device
            .create_image(&dst_image_create_info, None)
            .unwrap();

        let dst_memory_req = base.device.get_image_memory_requirements(dst_image);
        let dst_memory_index = find_memorytype_index(
            &dst_memory_req,
            &base.device_memory_properties,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .expect("Unable to find suitable memory index for depth image.");

        let dst_allocate_info = vk::MemoryAllocateInfo {
            allocation_size: dst_memory_req.size,
            memory_type_index: dst_memory_index,
            ..Default::default()
        };
        let dst_memory = base
            .device
            .allocate_memory(&dst_allocate_info, None)
            .unwrap();
        base.device
            .bind_image_memory(dst_image, dst_memory, 0)
            .expect("Unable to bind depth image memory");

        let mut dst_image_view_usage_create_info = vk::ImageViewUsageCreateInfo {
            usage: vk::ImageUsageFlags::VIDEO_DECODE_DST_KHR,
            ..Default::default()
        };

        let dst_image_view_info = vk::ImageViewCreateInfo {
            p_next: &mut dst_image_view_usage_create_info as *mut _ as _,
            view_type: vk::ImageViewType::TYPE_2D,
            format: dst_video_format,
            image: dst_image,
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                level_count: 1,
                layer_count: 1,
                ..Default::default()
            },
            ..Default::default()
        };

        let dst_image_view = base
            .device
            .create_image_view(&dst_image_view_info, None)
            .unwrap();
        
        // DPB
        
        let dpb_image_create_info = vk::ImageCreateInfo {
            p_next: &mut profile_list_info as *mut _ as _,
            image_type: vk::ImageType::TYPE_2D,
            format: dpb_video_format,
            extent: video_extent.into(),
            mip_levels: 1,
            array_layers: 1,
            samples: vk::SampleCountFlags::TYPE_1,
            tiling: vk::ImageTiling::OPTIMAL,
            usage: vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR,
            ..Default::default()
        };

        let dpb_image = base
            .device
            .create_image(&dpb_image_create_info, None)
            .unwrap();

        let dpb_memory_req = base.device.get_image_memory_requirements(dpb_image);
        let dpb_memory_index = find_memorytype_index(
            &dpb_memory_req,
            &base.device_memory_properties,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .expect("Unable to find suitable memory index for depth image.");

        let dpb_allocate_info = vk::MemoryAllocateInfo {
            allocation_size: dpb_memory_req.size,
            memory_type_index: dpb_memory_index,
            ..Default::default()
        };
        let dpb_memory = base
            .device
            .allocate_memory(&dpb_allocate_info, None)
            .unwrap();
        base.device
            .bind_image_memory(dpb_image, dpb_memory, 0)
            .expect("Unable to bind depth image memory");

        let mut dpb_image_view_usage_create_info = vk::ImageViewUsageCreateInfo {
            usage: vk::ImageUsageFlags::VIDEO_DECODE_DPB_KHR,
            ..Default::default()
        };

        let dpb_image_view_info = vk::ImageViewCreateInfo {
            p_next: &mut dpb_image_view_usage_create_info as *mut _ as _,
            view_type: vk::ImageViewType::TYPE_2D,
            format: dpb_video_format,
            image: dpb_image,
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                level_count: 1,
                layer_count: 1,
                ..Default::default()
            },
            ..Default::default()
        };

        let dpb_image_view = base
            .device
            .create_image_view(&dpb_image_view_info, None)
            .unwrap();
        */
        let renderpass_attachments = [
            vk::AttachmentDescription {
                format: base.surface_format.format,
                samples: vk::SampleCountFlags::TYPE_1,
                load_op: vk::AttachmentLoadOp::CLEAR,
                store_op: vk::AttachmentStoreOp::STORE,
                final_layout: vk::ImageLayout::PRESENT_SRC_KHR,
                ..Default::default()
            },
            vk::AttachmentDescription {
                format: vk::Format::D16_UNORM,
                samples: vk::SampleCountFlags::TYPE_1,
                load_op: vk::AttachmentLoadOp::CLEAR,
                initial_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                final_layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                ..Default::default()
            },
        ];
        let color_attachment_refs = [vk::AttachmentReference {
            attachment: 0,
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
        }];
        let depth_attachment_ref = vk::AttachmentReference {
            attachment: 1,
            layout: vk::ImageLayout::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
        };
        let dependencies = [vk::SubpassDependency {
            src_subpass: vk::SUBPASS_EXTERNAL,
            src_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            dst_access_mask: vk::AccessFlags::COLOR_ATTACHMENT_READ
                | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            dst_stage_mask: vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT,
            ..Default::default()
        }];

        let subpass = vk::SubpassDescription::default()
            .color_attachments(&color_attachment_refs)
            .depth_stencil_attachment(&depth_attachment_ref)
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS);

        let renderpass_create_info = vk::RenderPassCreateInfo::default()
            .attachments(&renderpass_attachments)
            .subpasses(std::slice::from_ref(&subpass))
            .dependencies(&dependencies);

        let renderpass = base
            .device
            .create_render_pass(&renderpass_create_info, None)
            .unwrap();

        let framebuffers: Vec<vk::Framebuffer> = base
            .present_image_views
            .iter()
            .map(|&present_image_view| {
                let framebuffer_attachments = [present_image_view, base.depth_image_view];
                let frame_buffer_create_info = vk::FramebufferCreateInfo::default()
                    .render_pass(renderpass)
                    .attachments(&framebuffer_attachments)
                    .width(base.surface_resolution.width)
                    .height(base.surface_resolution.height)
                    .layers(1);

                base.device
                    .create_framebuffer(&frame_buffer_create_info, None)
                    .unwrap()
            })
            .collect();
        let index_buffer_data = [0u32, 1, 2, 2, 3, 0];
        let index_buffer_info = vk::BufferCreateInfo {
            size: std::mem::size_of_val(&index_buffer_data) as u64,
            usage: vk::BufferUsageFlags::INDEX_BUFFER,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        };
        let index_buffer = base.device.create_buffer(&index_buffer_info, None).unwrap();
        let index_buffer_memory_req = base.device.get_buffer_memory_requirements(index_buffer);
        let index_buffer_memory_index = find_memorytype_index(
            &index_buffer_memory_req,
            &base.device_memory_properties,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .expect("Unable to find suitable memorytype for the index buffer.");
        let index_allocate_info = vk::MemoryAllocateInfo {
            allocation_size: index_buffer_memory_req.size,
            memory_type_index: index_buffer_memory_index,
            ..Default::default()
        };
        let index_buffer_memory = base
            .device
            .allocate_memory(&index_allocate_info, None)
            .unwrap();
        let index_ptr: *mut c_void = base
            .device
            .map_memory(
                index_buffer_memory,
                0,
                index_buffer_memory_req.size,
                vk::MemoryMapFlags::empty(),
            )
            .unwrap();
        let mut index_slice = Align::new(
            index_ptr,
            align_of::<u32>() as u64,
            index_buffer_memory_req.size,
        );
        index_slice.copy_from_slice(&index_buffer_data);
        base.device.unmap_memory(index_buffer_memory);
        base.device
            .bind_buffer_memory(index_buffer, index_buffer_memory, 0)
            .unwrap();

        let vertices = [
            Vertex {
                pos: [-1.0, -1.0, 0.0, 1.0],
                uv: [0.0, 0.0],
            },
            Vertex {
                pos: [-1.0, 1.0, 0.0, 1.0],
                uv: [0.0, 1.0],
            },
            Vertex {
                pos: [1.0, 1.0, 0.0, 1.0],
                uv: [1.0, 1.0],
            },
            Vertex {
                pos: [1.0, -1.0, 0.0, 1.0],
                uv: [1.0, 0.0],
            },
        ];
        let vertex_input_buffer_info = vk::BufferCreateInfo {
            size: std::mem::size_of_val(&vertices) as u64,
            usage: vk::BufferUsageFlags::VERTEX_BUFFER,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        };
        let vertex_input_buffer = base
            .device
            .create_buffer(&vertex_input_buffer_info, None)
            .unwrap();
        let vertex_input_buffer_memory_req = base
            .device
            .get_buffer_memory_requirements(vertex_input_buffer);
        let vertex_input_buffer_memory_index = find_memorytype_index(
            &vertex_input_buffer_memory_req,
            &base.device_memory_properties,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .expect("Unable to find suitable memorytype for the vertex buffer.");

        let vertex_buffer_allocate_info = vk::MemoryAllocateInfo {
            allocation_size: vertex_input_buffer_memory_req.size,
            memory_type_index: vertex_input_buffer_memory_index,
            ..Default::default()
        };
        let vertex_input_buffer_memory = base
            .device
            .allocate_memory(&vertex_buffer_allocate_info, None)
            .unwrap();

        let vert_ptr = base
            .device
            .map_memory(
                vertex_input_buffer_memory,
                0,
                vertex_input_buffer_memory_req.size,
                vk::MemoryMapFlags::empty(),
            )
            .unwrap();
        let mut slice = Align::new(
            vert_ptr,
            align_of::<Vertex>() as u64,
            vertex_input_buffer_memory_req.size,
        );
        slice.copy_from_slice(&vertices);
        base.device.unmap_memory(vertex_input_buffer_memory);
        base.device
            .bind_buffer_memory(vertex_input_buffer, vertex_input_buffer_memory, 0)
            .unwrap();

        let uniform_color_buffer_data = Vector3 {
            x: 1.0,
            y: 1.0,
            z: 1.0,
            _pad: 0.0,
        };
        let uniform_color_buffer_info = vk::BufferCreateInfo {
            size: std::mem::size_of_val(&uniform_color_buffer_data) as u64,
            usage: vk::BufferUsageFlags::UNIFORM_BUFFER,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        };
        let uniform_color_buffer = base
            .device
            .create_buffer(&uniform_color_buffer_info, None)
            .unwrap();
        let uniform_color_buffer_memory_req = base
            .device
            .get_buffer_memory_requirements(uniform_color_buffer);
        let uniform_color_buffer_memory_index = find_memorytype_index(
            &uniform_color_buffer_memory_req,
            &base.device_memory_properties,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .expect("Unable to find suitable memorytype for the vertex buffer.");

        let uniform_color_buffer_allocate_info = vk::MemoryAllocateInfo {
            allocation_size: uniform_color_buffer_memory_req.size,
            memory_type_index: uniform_color_buffer_memory_index,
            ..Default::default()
        };
        let uniform_color_buffer_memory = base
            .device
            .allocate_memory(&uniform_color_buffer_allocate_info, None)
            .unwrap();
        let uniform_ptr = base
            .device
            .map_memory(
                uniform_color_buffer_memory,
                0,
                uniform_color_buffer_memory_req.size,
                vk::MemoryMapFlags::empty(),
            )
            .unwrap();
        let mut uniform_aligned_slice = Align::new(
            uniform_ptr,
            align_of::<Vector3>() as u64,
            uniform_color_buffer_memory_req.size,
        );
        uniform_aligned_slice.copy_from_slice(&[uniform_color_buffer_data]);
        base.device.unmap_memory(uniform_color_buffer_memory);
        base.device
            .bind_buffer_memory(uniform_color_buffer, uniform_color_buffer_memory, 0)
            .unwrap();

        let image = image::load_from_memory(include_bytes!("../assets/rust.png"))
            .unwrap()
            .to_rgba8();
        let (width, height) = image.dimensions();
        let image_extent = vk::Extent2D { width, height };
        let image_data = image.into_raw();
        let image_buffer_info = vk::BufferCreateInfo {
            size: (std::mem::size_of::<u8>() * image_data.len()) as u64,
            usage: vk::BufferUsageFlags::TRANSFER_SRC,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        };
        let image_buffer = base.device.create_buffer(&image_buffer_info, None).unwrap();
        let image_buffer_memory_req = base.device.get_buffer_memory_requirements(image_buffer);
        let image_buffer_memory_index = find_memorytype_index(
            &image_buffer_memory_req,
            &base.device_memory_properties,
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT,
        )
        .expect("Unable to find suitable memorytype for the image buffer.");

        let image_buffer_allocate_info = vk::MemoryAllocateInfo {
            allocation_size: image_buffer_memory_req.size,
            memory_type_index: image_buffer_memory_index,
            ..Default::default()
        };
        let image_buffer_memory = base
            .device
            .allocate_memory(&image_buffer_allocate_info, None)
            .unwrap();
        let image_ptr = base
            .device
            .map_memory(
                image_buffer_memory,
                0,
                image_buffer_memory_req.size,
                vk::MemoryMapFlags::empty(),
            )
            .unwrap();
        let mut image_slice = Align::new(
            image_ptr,
            std::mem::align_of::<u8>() as u64,
            image_buffer_memory_req.size,
        );
        image_slice.copy_from_slice(&image_data);
        base.device.unmap_memory(image_buffer_memory);
        base.device
            .bind_buffer_memory(image_buffer, image_buffer_memory, 0)
            .unwrap();

        let texture_create_info = vk::ImageCreateInfo {
            image_type: vk::ImageType::TYPE_2D,
            format: vk::Format::R8G8B8A8_UNORM,
            extent: image_extent.into(),
            mip_levels: 1,
            array_layers: 1,
            samples: vk::SampleCountFlags::TYPE_1,
            tiling: vk::ImageTiling::OPTIMAL,
            usage: vk::ImageUsageFlags::TRANSFER_DST | vk::ImageUsageFlags::SAMPLED,
            sharing_mode: vk::SharingMode::EXCLUSIVE,
            ..Default::default()
        };
        let texture_image = base
            .device
            .create_image(&texture_create_info, None)
            .unwrap();
        let texture_memory_req = base.device.get_image_memory_requirements(texture_image);
        let texture_memory_index = find_memorytype_index(
            &texture_memory_req,
            &base.device_memory_properties,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
        )
        .expect("Unable to find suitable memory index for depth image.");

        let texture_allocate_info = vk::MemoryAllocateInfo {
            allocation_size: texture_memory_req.size,
            memory_type_index: texture_memory_index,
            ..Default::default()
        };
        let texture_memory = base
            .device
            .allocate_memory(&texture_allocate_info, None)
            .unwrap();
        base.device
            .bind_image_memory(texture_image, texture_memory, 0)
            .expect("Unable to bind depth image memory");

        record_submit_commandbuffer(
            &base.device,
            base.setup_command_buffer,
            base.setup_commands_reuse_fence,
            base.present_queue,
            &[],
            &[],
            &[],
            |device, texture_command_buffer| {
                let texture_barrier = vk::ImageMemoryBarrier {
                    dst_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    new_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    image: texture_image,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        level_count: 1,
                        layer_count: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                device.cmd_pipeline_barrier(
                    texture_command_buffer,
                    vk::PipelineStageFlags::BOTTOM_OF_PIPE,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[texture_barrier],
                );
                let buffer_copy_regions = vk::BufferImageCopy::default()
                    .image_subresource(
                        vk::ImageSubresourceLayers::default()
                            .aspect_mask(vk::ImageAspectFlags::COLOR)
                            .layer_count(1),
                    )
                    .image_extent(image_extent.into());

                device.cmd_copy_buffer_to_image(
                    texture_command_buffer,
                    image_buffer,
                    texture_image,
                    vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    &[buffer_copy_regions],
                );
                let texture_barrier_end = vk::ImageMemoryBarrier {
                    src_access_mask: vk::AccessFlags::TRANSFER_WRITE,
                    dst_access_mask: vk::AccessFlags::SHADER_READ,
                    old_layout: vk::ImageLayout::TRANSFER_DST_OPTIMAL,
                    new_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
                    image: texture_image,
                    subresource_range: vk::ImageSubresourceRange {
                        aspect_mask: vk::ImageAspectFlags::COLOR,
                        level_count: 1,
                        layer_count: 1,
                        ..Default::default()
                    },
                    ..Default::default()
                };
                device.cmd_pipeline_barrier(
                    texture_command_buffer,
                    vk::PipelineStageFlags::TRANSFER,
                    vk::PipelineStageFlags::FRAGMENT_SHADER,
                    vk::DependencyFlags::empty(),
                    &[],
                    &[],
                    &[texture_barrier_end],
                );
            },
        );

        let sampler_info = vk::SamplerCreateInfo {
            mag_filter: vk::Filter::LINEAR,
            min_filter: vk::Filter::LINEAR,
            mipmap_mode: vk::SamplerMipmapMode::LINEAR,
            address_mode_u: vk::SamplerAddressMode::MIRRORED_REPEAT,
            address_mode_v: vk::SamplerAddressMode::MIRRORED_REPEAT,
            address_mode_w: vk::SamplerAddressMode::MIRRORED_REPEAT,
            max_anisotropy: 1.0,
            border_color: vk::BorderColor::FLOAT_OPAQUE_WHITE,
            compare_op: vk::CompareOp::NEVER,
            ..Default::default()
        };

        let sampler = base.device.create_sampler(&sampler_info, None).unwrap();

        let tex_image_view_info = vk::ImageViewCreateInfo {
            view_type: vk::ImageViewType::TYPE_2D,
            format: texture_create_info.format,
            components: vk::ComponentMapping {
                r: vk::ComponentSwizzle::R,
                g: vk::ComponentSwizzle::G,
                b: vk::ComponentSwizzle::B,
                a: vk::ComponentSwizzle::A,
            },
            subresource_range: vk::ImageSubresourceRange {
                aspect_mask: vk::ImageAspectFlags::COLOR,
                level_count: 1,
                layer_count: 1,
                ..Default::default()
            },
            image: texture_image,
            ..Default::default()
        };
        let tex_image_view = base
            .device
            .create_image_view(&tex_image_view_info, None)
            .unwrap();
        let descriptor_sizes = [
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 1,
            },
            vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 1,
            },
        ];
        let descriptor_pool_info = vk::DescriptorPoolCreateInfo::default()
            .pool_sizes(&descriptor_sizes)
            .max_sets(1);

        let descriptor_pool = base
            .device
            .create_descriptor_pool(&descriptor_pool_info, None)
            .unwrap();
        let desc_layout_bindings = [
            vk::DescriptorSetLayoutBinding {
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                descriptor_count: 1,
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
                ..Default::default()
            },
            vk::DescriptorSetLayoutBinding {
                binding: 1,
                descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: 1,
                stage_flags: vk::ShaderStageFlags::FRAGMENT,
                ..Default::default()
            },
        ];
        let descriptor_info =
            vk::DescriptorSetLayoutCreateInfo::default().bindings(&desc_layout_bindings);

        let desc_set_layouts = [base
            .device
            .create_descriptor_set_layout(&descriptor_info, None)
            .unwrap()];

        let desc_alloc_info = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(descriptor_pool)
            .set_layouts(&desc_set_layouts);
        let descriptor_sets = base
            .device
            .allocate_descriptor_sets(&desc_alloc_info)
            .unwrap();

        let uniform_color_buffer_descriptor = vk::DescriptorBufferInfo {
            buffer: uniform_color_buffer,
            offset: 0,
            range: mem::size_of_val(&uniform_color_buffer_data) as u64,
        };

        let tex_descriptor = vk::DescriptorImageInfo {
            image_layout: vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            image_view: tex_image_view,
            sampler,
        };

        let write_desc_sets = [
            vk::WriteDescriptorSet {
                dst_set: descriptor_sets[0],
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::UNIFORM_BUFFER,
                p_buffer_info: &uniform_color_buffer_descriptor,
                ..Default::default()
            },
            vk::WriteDescriptorSet {
                dst_set: descriptor_sets[0],
                dst_binding: 1,
                descriptor_count: 1,
                descriptor_type: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                p_image_info: &tex_descriptor,
                ..Default::default()
            },
        ];
        base.device.update_descriptor_sets(&write_desc_sets, &[]);

        let mut vertex_spv_file = Cursor::new(&include_bytes!("../shader/texture/vert.spv")[..]);
        let mut frag_spv_file = Cursor::new(&include_bytes!("../shader/texture/frag.spv")[..]);

        let vertex_code =
            read_spv(&mut vertex_spv_file).expect("Failed to read vertex shader spv file");
        let vertex_shader_info = vk::ShaderModuleCreateInfo::default().code(&vertex_code);

        let frag_code =
            read_spv(&mut frag_spv_file).expect("Failed to read fragment shader spv file");
        let frag_shader_info = vk::ShaderModuleCreateInfo::default().code(&frag_code);

        let vertex_shader_module = base
            .device
            .create_shader_module(&vertex_shader_info, None)
            .expect("Vertex shader module error");

        let fragment_shader_module = base
            .device
            .create_shader_module(&frag_shader_info, None)
            .expect("Fragment shader module error");

        let layout_create_info =
            vk::PipelineLayoutCreateInfo::default().set_layouts(&desc_set_layouts);

        let pipeline_layout = base
            .device
            .create_pipeline_layout(&layout_create_info, None)
            .unwrap();

        let shader_entry_name = CStr::from_bytes_with_nul_unchecked(b"main\0");
        let shader_stage_create_infos = [
            vk::PipelineShaderStageCreateInfo {
                module: vertex_shader_module,
                p_name: shader_entry_name.as_ptr(),
                stage: vk::ShaderStageFlags::VERTEX,
                ..Default::default()
            },
            vk::PipelineShaderStageCreateInfo {
                module: fragment_shader_module,
                p_name: shader_entry_name.as_ptr(),
                stage: vk::ShaderStageFlags::FRAGMENT,
                ..Default::default()
            },
        ];
        let vertex_input_binding_descriptions = [vk::VertexInputBindingDescription {
            binding: 0,
            stride: mem::size_of::<Vertex>() as u32,
            input_rate: vk::VertexInputRate::VERTEX,
        }];
        let vertex_input_attribute_descriptions = [
            vk::VertexInputAttributeDescription {
                location: 0,
                binding: 0,
                format: vk::Format::R32G32B32A32_SFLOAT,
                offset: offset_of!(Vertex, pos) as u32,
            },
            vk::VertexInputAttributeDescription {
                location: 1,
                binding: 0,
                format: vk::Format::R32G32_SFLOAT,
                offset: offset_of!(Vertex, uv) as u32,
            },
        ];
        let vertex_input_state_info = vk::PipelineVertexInputStateCreateInfo::default()
            .vertex_attribute_descriptions(&vertex_input_attribute_descriptions)
            .vertex_binding_descriptions(&vertex_input_binding_descriptions);

        let vertex_input_assembly_state_info = vk::PipelineInputAssemblyStateCreateInfo {
            topology: vk::PrimitiveTopology::TRIANGLE_LIST,
            ..Default::default()
        };
        let viewports = [vk::Viewport {
            x: 0.0,
            y: 0.0,
            width: base.surface_resolution.width as f32,
            height: base.surface_resolution.height as f32,
            min_depth: 0.0,
            max_depth: 1.0,
        }];
        let scissors = [base.surface_resolution.into()];
        let viewport_state_info = vk::PipelineViewportStateCreateInfo::default()
            .scissors(&scissors)
            .viewports(&viewports);

        let rasterization_info = vk::PipelineRasterizationStateCreateInfo {
            front_face: vk::FrontFace::COUNTER_CLOCKWISE,
            line_width: 1.0,
            polygon_mode: vk::PolygonMode::FILL,
            ..Default::default()
        };

        let multisample_state_info = vk::PipelineMultisampleStateCreateInfo::default()
            .rasterization_samples(vk::SampleCountFlags::TYPE_1);

        let noop_stencil_state = vk::StencilOpState {
            fail_op: vk::StencilOp::KEEP,
            pass_op: vk::StencilOp::KEEP,
            depth_fail_op: vk::StencilOp::KEEP,
            compare_op: vk::CompareOp::ALWAYS,
            ..Default::default()
        };
        let depth_state_info = vk::PipelineDepthStencilStateCreateInfo {
            depth_test_enable: 1,
            depth_write_enable: 1,
            depth_compare_op: vk::CompareOp::LESS_OR_EQUAL,
            front: noop_stencil_state,
            back: noop_stencil_state,
            max_depth_bounds: 1.0,
            ..Default::default()
        };

        let color_blend_attachment_states = [vk::PipelineColorBlendAttachmentState {
            blend_enable: 0,
            src_color_blend_factor: vk::BlendFactor::SRC_COLOR,
            dst_color_blend_factor: vk::BlendFactor::ONE_MINUS_DST_COLOR,
            color_blend_op: vk::BlendOp::ADD,
            src_alpha_blend_factor: vk::BlendFactor::ZERO,
            dst_alpha_blend_factor: vk::BlendFactor::ZERO,
            alpha_blend_op: vk::BlendOp::ADD,
            color_write_mask: vk::ColorComponentFlags::RGBA,
        }];
        let color_blend_state = vk::PipelineColorBlendStateCreateInfo::default()
            .logic_op(vk::LogicOp::CLEAR)
            .attachments(&color_blend_attachment_states);

        let dynamic_state = [vk::DynamicState::VIEWPORT, vk::DynamicState::SCISSOR];
        let dynamic_state_info =
            vk::PipelineDynamicStateCreateInfo::default().dynamic_states(&dynamic_state);

        let graphic_pipeline_infos = vk::GraphicsPipelineCreateInfo::default()
            .stages(&shader_stage_create_infos)
            .vertex_input_state(&vertex_input_state_info)
            .input_assembly_state(&vertex_input_assembly_state_info)
            .viewport_state(&viewport_state_info)
            .rasterization_state(&rasterization_info)
            .multisample_state(&multisample_state_info)
            .depth_stencil_state(&depth_state_info)
            .color_blend_state(&color_blend_state)
            .dynamic_state(&dynamic_state_info)
            .layout(pipeline_layout)
            .render_pass(renderpass);

        let graphics_pipelines = base
            .device
            .create_graphics_pipelines(vk::PipelineCache::null(), &[graphic_pipeline_infos], None)
            .unwrap();

        let graphic_pipeline = graphics_pipelines[0];

        base.render_loop(|| {
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
                    /*
                    video_queue_loader.cmd_begin_video_coding(
                        draw_command_buffer,
                        &vk::VideoBeginCodingInfoKHR::default(),
                    );

                    let decode_output_picture_resource = vk::VideoPictureResourceInfoKHR {
                        base_array_layer: 0,
                        image_view_binding: dst_image_view,
                        ..Default::default()
                    };

                    let decode_info = vk::VideoDecodeInfoKHR {
                        src_buffer: bistream_buffer,
                        dst_picture_resource: decode_output_picture_resource,
                        reference_slot_count: 0,
                        ..Default::default()
                    };

                    video_decode_queue_loader.cmd_decode_video(
                        draw_command_buffer,
                        &decode_info,
                    );

                    video_queue_loader.cmd_end_video_coding(
                        draw_command_buffer,
                        &vk::VideoEndCodingInfoKHR::default(),
                    );
                    */
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
        });
        base.device.device_wait_idle().unwrap();

        for pipeline in graphics_pipelines {
            base.device.destroy_pipeline(pipeline, None);
        }
        base.device.destroy_pipeline_layout(pipeline_layout, None);
        base.device
            .destroy_shader_module(vertex_shader_module, None);
        base.device
            .destroy_shader_module(fragment_shader_module, None);
        /*
        base.device.destroy_buffer(bistream_buffer, None);
        base.device.free_memory(dpb_memory, None);
        base.device.destroy_image(dpb_image, None);
        base.device.destroy_image_view(dpb_image_view, None);
        base.device.free_memory(dst_memory, None);
        base.device.destroy_image(dst_image, None);
        base.device.destroy_image_view(dst_image_view, None);
        */
        base.device.free_memory(image_buffer_memory, None);
        base.device.destroy_buffer(image_buffer, None);
        base.device.free_memory(texture_memory, None);
        base.device.destroy_image_view(tex_image_view, None);
        base.device.destroy_image(texture_image, None);
        base.device.free_memory(index_buffer_memory, None);
        base.device.destroy_buffer(index_buffer, None);
        base.device.free_memory(uniform_color_buffer_memory, None);
        base.device.destroy_buffer(uniform_color_buffer, None);
        base.device.free_memory(vertex_input_buffer_memory, None);
        base.device.destroy_buffer(vertex_input_buffer, None);
        for &descriptor_set_layout in desc_set_layouts.iter() {
            base.device
                .destroy_descriptor_set_layout(descriptor_set_layout, None);
        }
        base.device.destroy_descriptor_pool(descriptor_pool, None);
        base.device.destroy_sampler(sampler, None);
        for framebuffer in framebuffers {
            base.device.destroy_framebuffer(framebuffer, None);
        }
        base.device.destroy_render_pass(renderpass, None);
    }

    Ok(())
}
