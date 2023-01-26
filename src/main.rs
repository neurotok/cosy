use std::{
    env,
    io::{Cursor, Read},
};

use anyhow::Result;

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use mp4parse;

use cosy::*;

// https://github.com/mozilla/mp4parse-rust/blob/a4329008c588401b1cfc283690a0118775dea728/mp4parse/tests/public.rs

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

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();


    let mut file = std::fs::File::open({
        if DEBUG_ENABLED {
            "/home/marcin/git/cozy/samples/Big_Buck_Bunny_360_10s_1MB.mp4"
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

    let event_loop = EventLoop::new();

    let window = WindowBuilder::new()
        .with_title("Cosy player")
        .with_inner_size(winit::dpi::LogicalSize::new(f64::from(800), f64::from(600)))
        .build(&event_loop)
        .unwrap();

    let mut app = unsafe { App::create(&window)? };

    let mut destroying = false;
    event_loop.run(move |event, _, control_flow| {
        *control_flow = ControlFlow::Poll;
        match event {
            Event::MainEventsCleared if !destroying => unsafe { app.render(&window) }.unwrap(),
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => {
                destroying = true;
                *control_flow = ControlFlow::Exit;
                unsafe {
                    app.destroy();
                }
            }
            _ => {}
        }
    });
}
