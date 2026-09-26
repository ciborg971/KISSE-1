#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

#[cfg(all(feature = "qualia", feature = "astronaut"))]
compile_error!("Enable only one board feature: `qualia` or `astronaut`.");
#[cfg(not(any(feature = "qualia", feature = "astronaut")))]
compile_error!("Enable one board feature: `qualia` or `astronaut`.");

use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::peripherals::Peripherals;
use esp_hal::timer::timg::TimerGroup;

use astronaut::display;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.4.0
    // generator parameters: -o esp32s3 -o esp32s3-wroom-1-octal-psram -o alloc -o unstable-hal -o embassy

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    // TODO: Spawn some tasks
    let _ = spawner;

    #[cfg(feature = "qualia")]
    run_qualia(peripherals).await;

    #[cfg(feature = "astronaut")]
    run_astronaut(peripherals).await;

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.2.2/examples
}

/// State of the UP / DOWN buttons (true = pressed).
type Buttons = (bool, bool);

/// UI loop shared by both boards.
async fn run_ui(mut fb: display::Framebuffer, mut read_buttons: impl FnMut() -> Buttons) -> ! {
    use slint::ComponentHandle;

    let window = display::ui::init();
    let app = astronaut::AppWindow::new().unwrap();
    app.show().unwrap();

    let mut counter = 0;
    let (mut was_up, mut was_dn) = (false, false);
    loop {
        let (up, dn) = read_buttons();
        if up && !was_up {
            counter += 1;
        }
        if dn && !was_dn {
            counter -= 1;
        }
        (was_up, was_dn) = (up, dn);

        app.set_counter(counter);
        app.set_up_pressed(up);
        app.set_down_pressed(dn);

        slint::platform::update_timers_and_animations();
        display::ui::draw(&window, &mut fb);

        Timer::after(Duration::from_millis(20)).await;
    }
}

/// Give PSRAM to the display framebuffer (at the start) and the heap (the rest).
fn split_psram(psram: &esp_hal::psram::Psram) -> display::Framebuffer {
    let (start, size) = psram.raw_parts();
    assert!(size > display::FB_BYTES, "PSRAM not found");
    // SAFETY: start of PSRAM, only used for the framebuffer (the heap gets what follows).
    let fb = unsafe { display::Framebuffer::from_psram(start) };
    unsafe {
        esp_alloc::HEAP.add_region(esp_alloc::HeapRegion::new(
            start.add(display::FB_BYTES),
            size - display::FB_BYTES,
            esp_alloc::MemoryCapability::External.into(),
        ));
    }
    fb
}

#[cfg(feature = "qualia")]
#[allow(
    clippy::large_stack_frames,
    reason = "board setup holds the peripheral singletons and drivers"
)]
async fn run_qualia(peripherals: Peripherals) -> ! {
    use astronaut::board::qualia::ExpanderPanelBus;
    use esp_hal::{
        delay::Delay,
        i2c::master::{Config as I2cConfig, I2c},
        lcd_cam::{LcdCam, lcd::dpi::Dpi},
        psram::{Psram, PsramConfig, PsramMode, SpiRamFreq},
    };
    use port_expander::dev::pca9554::Pca9554A;

    // The following pins are used to bootstrap the chip. They are available
    // for use, but check the datasheet of the module for more information on them.
    // - GPIO0
    // - GPIO3
    // - GPIO45
    // - GPIO46
    // These GPIO pins are in use by some feature of the module and should not be used.
    let _gpio27 = peripherals.GPIO27;
    let _gpio28 = peripherals.GPIO28;
    let _gpio29 = peripherals.GPIO29;
    let _gpio30 = peripherals.GPIO30;
    let _gpio31 = peripherals.GPIO31;
    let _gpio32 = peripherals.GPIO32;
    let _gpio33 = peripherals.GPIO33;
    let _gpio34 = peripherals.GPIO34;
    let _gpio35 = peripherals.GPIO35;
    let _gpio36 = peripherals.GPIO36;
    let _gpio37 = peripherals.GPIO37;

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);

    // 8 MB octal PSRAM.
    let psram = Psram::new(
        peripherals.PSRAM,
        PsramConfig {
            mode: PsramMode::OctalSpi,
            ram_frequency: SpiRamFreq::Freq80m,
            ..Default::default()
        },
    );
    let fb = split_psram(&psram);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    // On-board I2C bus (SDA = GPIO8, SCL = GPIO18) with the GPIO expander at 0x3F.
    let mut i2c = I2c::new(peripherals.I2C0, I2cConfig::default())
        .unwrap()
        .with_sda(peripherals.GPIO8)
        .with_scl(peripherals.GPIO18);

    // Configure the panel through the expander, then start streaming the framebuffer.
    display::panel::init(&mut ExpanderPanelBus::new(&mut i2c).unwrap(), &mut Delay::new()).unwrap();

    let lcd_cam = LcdCam::new(peripherals.LCD_CAM);
    let dpi = Dpi::new(lcd_cam.lcd, peripherals.DMA_CH0, display::dpi::config())
        .unwrap()
        .with_vsync(peripherals.GPIO42)
        .with_hsync(peripherals.GPIO41)
        .with_de(peripherals.GPIO2)
        .with_pclk(peripherals.GPIO1)
        // Blue
        .with_data0(peripherals.GPIO40)
        .with_data1(peripherals.GPIO39)
        .with_data2(peripherals.GPIO38)
        .with_data3(peripherals.GPIO0)
        .with_data4(peripherals.GPIO45)
        // Green
        .with_data5(peripherals.GPIO48)
        .with_data6(peripherals.GPIO47)
        .with_data7(peripherals.GPIO21)
        .with_data8(peripherals.GPIO14)
        .with_data9(peripherals.GPIO13)
        .with_data10(peripherals.GPIO12)
        // Red
        .with_data11(peripherals.GPIO11)
        .with_data12(peripherals.GPIO10)
        .with_data13(peripherals.GPIO9)
        .with_data14(peripherals.GPIO46)
        .with_data15(peripherals.GPIO3);
    // Runs forever; dropping it would stop the display.
    let _refresh = dpi
        .send(true, display::bounce::BounceRing::new(&fb))
        .map_err(|e| e.0)
        .unwrap();
    display::bounce::start_refill();

    // PCA9554A address = 0x38 | A2 A1 A0 -> 0x3F with all address pins high.
    let mut expander = Pca9554A::new(i2c, true, true, true);
    let expander_pins = expander.split();
    let btn_up = expander_pins.io5;
    let btn_dn = expander_pins.io6;

    // Buttons pull to ground when pressed.
    run_ui(fb, || {
        (
            btn_up.is_low().unwrap_or(false),
            btn_dn.is_low().unwrap_or(false),
        )
    })
    .await
}

#[cfg(feature = "astronaut")]
#[allow(
    clippy::large_stack_frames,
    reason = "board setup holds the peripheral singletons and drivers"
)]
async fn run_astronaut(peripherals: Peripherals) -> ! {
    use esp_hal::{
        gpio::{Input, InputConfig, Pull},
        psram::Psram,
    };

    esp_alloc::heap_allocator!(size: 65536);

    // 32 MB PSRAM on the ESP32-P4 module.
    let psram = Psram::new(peripherals.PSRAM, Default::default());
    // TODO: scan `fb` out to the panel (e.g. with `display::FrameRing`) once esp-hal has an RGB
    // driver for the ESP32-P4 (pins in `board::astronaut`).
    let fb = split_psram(&psram);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    // BUTTON1 as UP (assumed to pull to ground when pressed).
    let button1 = Input::new(peripherals.GPIO35, InputConfig::default().with_pull(Pull::Up));

    run_ui(fb, || (button1.is_low(), false)).await
}
