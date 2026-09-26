//! Bounce-buffer scan-out (ESP32-S3).
//!
//! DMA straight from PSRAM to the RGB peripheral drifts on the ESP32-S3: whenever PSRAM can't
//! keep up, the panel loses pixels and the image rolls. As in ESP-IDF's bounce-buffer mode,
//! the DMA only reads two small internal-RAM buffers ("halves" of [`LINES_PER_HALF`] lines),
//! and the DMA EOF interrupt refills the half that was just sent from the PSRAM framebuffer.
//!
//! The descriptor ring covers exactly one frame (alternating A/B halves) and loops, so each
//! pass of the ring is one panel frame. The half to refill is derived from the hardware
//! `out_eof_des_addr` register rather than a counter, so a late interrupt cannot shift the
//! image permanently.
//!
//! Adapted from the `ws-esp32-s3-touch-lcd-2-8c` crate (MIT, Gareth Bowker).

use core::sync::atomic::{AtomicPtr, Ordering};

use esp_hal::{
    dma::{BurstConfig, DmaDescriptor, DmaTxBuffer, Owner, Preparation},
    handler,
    interrupt::{self, Priority},
    peripherals::{DMA, Interrupt},
};

use super::{FB_BYTES, Framebuffer, HEIGHT, LINE};

/// Lines per bounce half. `HEIGHT / LINES_PER_HALF` must be even.
pub const LINES_PER_HALF: usize = 16;
const HALF_BYTES: usize = LINES_PER_HALF * LINE * 2;
const HALVES_PER_FRAME: usize = HEIGHT / LINES_PER_HALF;
/// <= 4095 bytes per descriptor.
const CHUNK: usize = 4032;
const DESC_PER_HALF: usize = HALF_BYTES.div_ceil(CHUNK);
const N_DESC: usize = DESC_PER_HALF * HALVES_PER_FRAME;

/// Shifts the image down by this many lines, to compensate a constant vertical offset
/// between DMA and panel start if one shows up.
const START_OFFSET_LINES: usize = 0;
const START_OFFSET_BYTES: usize = START_OFFSET_LINES * LINE * 2;

/// DMA channel used for the RGB peripheral (`DMA_CH0` in `main.rs`).
const DMA_CHANNEL: usize = 0;

const _: () = assert!(HEIGHT.is_multiple_of(LINES_PER_HALF));
const _: () = assert!(HALVES_PER_FRAME.is_multiple_of(2));

#[repr(C, align(64))]
struct Half([u8; HALF_BYTES]);

static mut BOUNCE_A: Half = Half([0; HALF_BYTES]);
static mut BOUNCE_B: Half = Half([0; HALF_BYTES]);
static mut DESCRIPTORS: [DmaDescriptor; N_DESC] = [DmaDescriptor::EMPTY; N_DESC];

/// PSRAM framebuffer the interrupt copies from.
static SOURCE: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut());

/// Descriptor ring over the two bounce halves, to pass to `Dpi::send(true, ..)`.
pub struct BounceRing {
    head: *mut DmaDescriptor,
}

impl BounceRing {
    /// Build the ring and prime both halves from `fb`. Call once, then start the transfer and
    /// call [`start_refill`].
    pub fn new(fb: &Framebuffer) -> Self {
        let descs = core::ptr::addr_of_mut!(DESCRIPTORS) as *mut DmaDescriptor;
        let (a, b) = halves();

        for half in 0..HALVES_PER_FRAME {
            let buf = if half.is_multiple_of(2) { a } else { b };
            for i in 0..DESC_PER_HALF {
                let start = i * CHUNK;
                let len = CHUNK.min(HALF_BYTES - start);
                let index = half * DESC_PER_HALF + i;
                // SAFETY: index < N_DESC, the ring is only touched here before DMA starts.
                unsafe {
                    let d = &mut *descs.add(index);
                    d.set_size(len);
                    d.set_length(len);
                    d.buffer = buf.add(start);
                    d.set_owner(Owner::Dma);
                    // EOF at the end of each half triggers the refill interrupt.
                    d.set_suc_eof(i == DESC_PER_HALF - 1);
                    d.next = descs.add((index + 1) % N_DESC);
                }
            }
        }

        let source = fb.as_ptr() as *mut u8;
        SOURCE.store(source, Ordering::Release);
        // SAFETY: the halves are not in use by the DMA yet.
        unsafe {
            copy_half(source, 0, a);
            copy_half(source, 1, b);
        }

        Self { head: descs }
    }
}

unsafe impl DmaTxBuffer for BounceRing {
    type View = Self;
    type Final = Self;

    fn prepare(&mut self) -> Preparation {
        Preparation {
            start: self.head,
            // The DMA only reads the internal-RAM halves.
            accesses_psram: false,
            burst_transfer: BurstConfig::default(),
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

/// Enable the DMA EOF interrupt that refills the halves. Call right after `Dpi::send`.
pub fn start_refill() {
    interrupt::bind_handler(Interrupt::DMA_OUT_CH0, refill);
    DMA::regs()
        .ch(DMA_CHANNEL)
        .out_int()
        .ena()
        .modify(|_, w| w.out_eof().set_bit());
}

#[handler(priority = Priority::Priority3)]
fn refill() {
    let ch = DMA::regs().ch(DMA_CHANNEL);
    if ch.out_int().st().read().out_eof().bit_is_clear() {
        return;
    }
    ch.out_int().clr().write(|w| w.out_eof().clear_bit_by_one());

    // Which descriptor raised the latest EOF, hence which half was just sent.
    let eof = ch.out_eof_des_addr().read().bits() as usize;
    let base = core::ptr::addr_of!(DESCRIPTORS) as usize;
    let size = core::mem::size_of::<DmaDescriptor>();
    if eof < base || eof >= base + N_DESC * size {
        return;
    }
    let finished = (eof - base) / size / DESC_PER_HALF;

    // That buffer is sent again two halves later.
    let (a, b) = halves();
    let dest = if finished.is_multiple_of(2) { a } else { b };
    let next = (finished + 2) % HALVES_PER_FRAME;
    // SAFETY: the DMA is now reading the other half.
    unsafe { copy_half(SOURCE.load(Ordering::Acquire), next, dest) };
}

fn halves() -> (*mut u8, *mut u8) {
    (
        core::ptr::addr_of_mut!(BOUNCE_A) as *mut u8,
        core::ptr::addr_of_mut!(BOUNCE_B) as *mut u8,
    )
}

/// Copy half number `index` of the frame from `source` to `dest`.
///
/// # Safety
///
/// `source` must be a [`FB_BYTES`] framebuffer and `dest` one of the bounce halves.
unsafe fn copy_half(source: *const u8, index: usize, dest: *mut u8) {
    let start = (index * HALF_BYTES + FB_BYTES - START_OFFSET_BYTES) % FB_BYTES;
    let first = HALF_BYTES.min(FB_BYTES - start);
    unsafe {
        core::ptr::copy_nonoverlapping(source.add(start), dest, first);
        core::ptr::copy_nonoverlapping(source, dest.add(first), HALF_BYTES - first);
    }
}
