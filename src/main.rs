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

fn main() -> Result<()> {
    let args: Vec<String> = env::args().collect();

    // Debug
    //let file = &args[1];
    let file = "/home/marcin/git/cozy/samples/Big_Buck_Bunny_360_10s_1MB.mp4".to_owned();

    let mut file = std::fs::File::open(file)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    let mut c = Cursor::new(&buf);
    let video_context = mp4parse::read_mp4(&mut c)?;
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
                //Debug
                assert_eq!(v.width, 640);
                assert_eq!(v.height, 360);

                match v.codec_specific {
                    mp4parse::VideoCodecSpecific::AVCConfig(ref avc) => {
                        assert!(!avc.is_empty());
                    }
                    _ => panic!("expected an AVC sampleEntry"),
                }
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
