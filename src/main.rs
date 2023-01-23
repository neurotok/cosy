use anyhow::Result;

use winit::{
    event::{Event, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::WindowBuilder,
};

use cosy::*;

fn main() -> Result<()> {
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
