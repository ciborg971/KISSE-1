//! Adafruit 4.58" 320x960 RGB bar display (product 5805, HD458002C40 / ST7701S), shared by
//! the Qualia and Astronaut boards.
//!
//! One RGB565 framebuffer lives in PSRAM; the UI draws into it and the RGB peripheral scans it
//! out continuously, either through internal-RAM bounce buffers ([`bounce`], ESP32-S3) or with
//! DMA reading PSRAM directly ([`FrameRing`]). [`Framebuffer::flush`] makes CPU writes visible
//! to the DMA, which bypasses the cache.
//!
//! Timings are from Adafruit's `adafruit_qualia.displays.bar320x960`. The panel's line is
//! 400 pixels long; the first [`OVERSCAN_LEFT`] pixels are not visible.

#[cfg(feature = "qualia")]
pub mod bounce;
#[cfg(feature = "qualia")]
pub mod dpi;
pub mod panel;
pub mod ui;

use esp_hal::dma::{
    BurstConfig,
    DmaDescriptor,
    DmaTxBuffer,
    ExternalBurstConfig,
    InternalBurstConfig,
    Owner,
    Preparation,
};

/// Visible width in pixels.
pub const WIDTH: usize = 320;
/// Visible height in pixels.
pub const HEIGHT: usize = 960;
/// Invisible pixels at the start of each line.
pub const OVERSCAN_LEFT: usize = 80;
/// Pixels per line sent to the panel (visible + overscan, multiple of 16).
pub const LINE: usize = (WIDTH + OVERSCAN_LEFT).div_ceil(16) * 16;
/// Framebuffer size in bytes (RGB565).
pub const FB_BYTES: usize = LINE * HEIGHT * 2;

/// Pixel clock in MHz (~33 Hz refresh).
pub const PCLK_MHZ: u32 = 16;
pub const HSYNC_PULSE: usize = 10;
pub const HSYNC_BACK_PORCH: usize = 30;
pub const HSYNC_FRONT_PORCH: usize = 50;
pub const VSYNC_PULSE: usize = 2;
pub const VSYNC_BACK_PORCH: usize = 17;
pub const VSYNC_FRONT_PORCH: usize = 14;

/// DMA chunk per descriptor: <= 4095 and a multiple of the 64-byte PSRAM burst.
const CHUNK: usize = 4032;
const N_DESC: usize = FB_BYTES.div_ceil(CHUNK);

const _: () = assert!(FB_BYTES.is_multiple_of(64));

// Descriptors must live in internal RAM.
static mut DESCRIPTORS: [DmaDescriptor; N_DESC] = [DmaDescriptor::EMPTY; N_DESC];

/// Closed descriptor ring over the whole framebuffer, so the DMA loops frame after frame.
pub struct FrameRing {
    head: *mut DmaDescriptor,
}

impl FrameRing {
    /// Ring for DMA reading `fb` directly from PSRAM.
    ///
    /// # Safety
    ///
    /// Must only be called once.
    pub unsafe fn new(fb: &Framebuffer) -> Self {
        let fb = fb.as_ptr() as *mut u8;
        let descs = core::ptr::addr_of_mut!(DESCRIPTORS) as *mut DmaDescriptor;
        for i in 0..N_DESC {
            let start = i * CHUNK;
            let len = CHUNK.min(FB_BYTES - start);
            unsafe {
                let d = &mut *descs.add(i);
                d.set_size(len);
                d.set_length(len);
                d.buffer = fb.add(start);
                d.set_owner(Owner::Dma);
                d.set_suc_eof(i == N_DESC - 1);
                d.next = descs.add((i + 1) % N_DESC);
            }
        }
        Self { head: descs }
    }
}

unsafe impl DmaTxBuffer for FrameRing {
    type View = Self;
    type Final = Self;

    fn prepare(&mut self) -> Preparation {
        Preparation {
            start: self.head,
            accesses_psram: true,
            burst_transfer: BurstConfig {
                external_memory: ExternalBurstConfig::Size64,
                internal_memory: InternalBurstConfig::Enabled,
            },
            // The owner bit is never handed back in a loop.
            check_owner: Some(false),
            auto_write_back: false,
        }
    }

    fn into_view(self) -> Self::View {
        self
    }

    fn from_view(view: Self::View) -> Self {
        view
    }
}

/// CPU-side access to the PSRAM framebuffer.
pub struct Framebuffer {
    pixels: &'static mut [u16],
}

impl Framebuffer {
    /// Carve the framebuffer from the start of PSRAM, cleared to black.
    ///
    /// # Safety
    ///
    /// `psram` must be the start of mapped PSRAM (64-byte aligned) with at least
    /// [`FB_BYTES`] available, not used by anything else. Must only be called once.
    pub unsafe fn from_psram(psram: *mut u8) -> Self {
        let pixels = unsafe { core::slice::from_raw_parts_mut(psram as *mut u16, LINE * HEIGHT) };
        pixels.fill(0);
        let mut fb = Self { pixels };
        fb.flush(0..HEIGHT);
        fb
    }

    /// Start of the framebuffer (including the overscan pixels).
    pub fn as_ptr(&self) -> *const u8 {
        self.pixels.as_ptr() as *const u8
    }

    /// Visible pixels of row `y`, in RGB565.
    pub fn row(&mut self, y: usize) -> &mut [u16] {
        let start = y * LINE + OVERSCAN_LEFT;
        &mut self.pixels[start..start + WIDTH]
    }

    /// Write rows `lines` back from the CPU cache to PSRAM so the DMA sends them.
    pub fn flush(&mut self, lines: core::ops::Range<usize>) {
        let lines = lines.start.min(HEIGHT)..lines.end.min(HEIGHT);
        if lines.is_empty() {
            return;
        }
        let start = self.pixels[lines.start * LINE..].as_ptr() as u32;
        let size = (lines.len() * LINE * 2) as u32;
        unsafe { cache_writeback(start, size) };
    }
}

/// Write back a PSRAM range of the data cache (ROM routines, as used by esp-hal).
#[cfg(feature = "qualia")]
unsafe fn cache_writeback(addr: u32, size: u32) {
    unsafe extern "C" {
        fn rom_Cache_WriteBack_Addr(addr: u32, size: u32);
        fn Cache_Suspend_DCache_Autoload() -> u32;
        fn Cache_Resume_DCache_Autoload(value: u32);
    }
    unsafe {
        let autoload = Cache_Suspend_DCache_Autoload();
        rom_Cache_WriteBack_Addr(addr, size);
        Cache_Resume_DCache_Autoload(autoload);
    }
}

/// Write back a PSRAM range of the L1 data cache and L2 cache (ROM routine, as used by esp-hal).
#[cfg(feature = "astronaut")]
unsafe fn cache_writeback(addr: u32, size: u32) {
    const CACHE_MAP_L1_DCACHE: u32 = 1 << 4;
    const CACHE_MAP_L2_CACHE: u32 = 1 << 5;
    unsafe extern "C" {
        fn Cache_WriteBack_Addr(bus: u32, addr: u32, size: u32);
    }
    unsafe { Cache_WriteBack_Addr(CACHE_MAP_L1_DCACHE | CACHE_MAP_L2_CACHE, addr, size) };
}
