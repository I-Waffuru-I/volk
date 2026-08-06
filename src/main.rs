// SPDX-License-Identifier: Apache-2.0

#![allow(
    dead_code,
    unsafe_op_in_unsafe_fn,
    unused_variables,
    clippy::manual_slice_size_calculation,
    clippy::too_many_arguments,
    clippy::unnecessary_wraps
)]


use anyhow::Result;
use log::*;
use winit::dpi::LogicalSize;
use winit::event::{Event, WindowEvent};
use winit::event_loop::EventLoop;
use winit::window::WindowBuilder;
use thiserror::Error;

mod app;
mod consts;


#[derive(Debug, Error)]
#[error("Missing {0}.")]
pub struct SuitabilityError(pub &'static str);


#[rustfmt::skip]
fn main() -> Result<()> {
    pretty_env_logger::init();
    _ = log_enabled!(Level::Info);

    // Window
    let event_loop = EventLoop::new()?;
    let window = WindowBuilder::new()
        .with_title("Vulkan Tutorial (Rust)")
        .with_inner_size(LogicalSize::new(1024, 768))
        .build(&event_loop)?;

    // App
    let mut app = unsafe { app::App::create(&window)? };
    let mut minimized = false;

    info!("Starting main loop");
    event_loop.run(move |event, elwt| {
        match event {
            // Request a redraw when all events were processed.
            Event::AboutToWait => window.request_redraw(),
            Event::WindowEvent {event, .. } => match event {
                WindowEvent::Resized(size) => {
                    if size.width == 0 || size.height == 0 {
                        minimized = true;
                    } else {
                        minimized = false;
                        app.resized = true;
                    }
                }
                // Render a frame if our Vulkan app is not being destroyed.or minimized
                WindowEvent::RedrawRequested if !elwt.exiting() && !minimized => {
                    if let Err(e) = unsafe { app.render(&window) } {
                        error!("Render error, rebuilding App: {e}");
                        unsafe {
                            app.destroy();
                            app = app::App::create(&window).expect("Failed to recreate App after device loss");
                        }
                    }
                }
                // Destroy our Vulkan app.
                WindowEvent::CloseRequested => {
                    elwt.exit();
                    unsafe { app.destroy(); }
                }
                _ => {}
            }
            _ => {}
        }
    })?;

    Ok(())
}



