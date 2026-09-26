//! Slint platform: software renderer drawing line by line into the PSRAM framebuffer.

use alloc::{boxed::Box, rc::Rc};
use core::ops::Range;

use slint::platform::{
    Platform,
    PlatformError,
    WindowAdapter,
    software_renderer::{LineBufferProvider, MinimalSoftwareWindow, RepaintBufferType, Rgb565Pixel},
};

use super::{Framebuffer, HEIGHT, WIDTH};

struct DisplayPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for DisplayPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }

    fn duration_since_start(&self) -> core::time::Duration {
        core::time::Duration::from_micros(embassy_time::Instant::now().as_micros())
    }
}

/// Install the Slint platform. Call once, before creating any Slint component.
pub fn init() -> Rc<MinimalSoftwareWindow> {
    // The framebuffer is on screen and persists between frames: only redraw what changed.
    let window = MinimalSoftwareWindow::new(RepaintBufferType::ReusedBuffer);
    window.set_size(slint::PhysicalSize::new(WIDTH as u32, HEIGHT as u32));
    slint::platform::set_platform(Box::new(DisplayPlatform {
        window: window.clone(),
    }))
    .expect("Slint platform already set");
    window
}

/// Render the pending changes of `window` (if any) into the framebuffer.
pub fn draw(window: &MinimalSoftwareWindow, fb: &mut Framebuffer) {
    window.draw_if_needed(|renderer| {
        let mut target = Target { fb, dirty: None };
        renderer.render_by_line(&mut target);
        if let Some(dirty) = target.dirty {
            target.fb.flush(dirty);
        }
    });
}

struct Target<'a> {
    fb: &'a mut Framebuffer,
    /// Rows written during this draw.
    dirty: Option<Range<usize>>,
}

impl LineBufferProvider for &mut Target<'_> {
    type TargetPixel = Rgb565Pixel;

    fn process_line(
        &mut self,
        line: usize,
        range: Range<usize>,
        render_fn: impl FnOnce(&mut [Self::TargetPixel]),
    ) {
        let row = &mut self.fb.row(line)[range];
        // SAFETY: Rgb565Pixel is a transparent wrapper around the u16 RGB565 value.
        let row = unsafe {
            core::slice::from_raw_parts_mut(row.as_mut_ptr() as *mut Rgb565Pixel, row.len())
        };
        render_fn(row);
        self.dirty = Some(match self.dirty.take() {
            Some(d) => d.start.min(line)..d.end.max(line + 1),
            None => line..line + 1,
        });
    }
}
