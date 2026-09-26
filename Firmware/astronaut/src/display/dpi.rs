//! RGB/DPI output configuration (ESP32-S3 LCD_CAM).
//!
//! esp-hal 1.2 has no RGB display driver for the ESP32-P4 yet, so this is Qualia-only.

use esp_hal::{
    gpio::Level,
    lcd_cam::lcd::{
        ClockMode,
        Phase,
        Polarity,
        dpi::{Config, Format, FrameTiming},
    },
    time::Rate,
};

use super::*;

/// DPI configuration for the panel.
pub fn config() -> Config {
    Config::default()
        // Panel samples on the falling edge: shift data out on the rising edge.
        .with_clock_mode(ClockMode {
            polarity: Polarity::IdleLow,
            phase: Phase::ShiftHigh,
        })
        .with_frequency(Rate::from_mhz(PCLK_MHZ))
        .with_format(Format {
            enable_2byte_mode: true,
            ..Default::default()
        })
        .with_timing(FrameTiming {
            horizontal_active_width: LINE,
            horizontal_total_width: HSYNC_PULSE + HSYNC_BACK_PORCH + LINE + HSYNC_FRONT_PORCH,
            horizontal_blank_front_porch: HSYNC_PULSE + HSYNC_BACK_PORCH,

            vertical_active_height: HEIGHT,
            vertical_total_height: VSYNC_PULSE + VSYNC_BACK_PORCH + HEIGHT + VSYNC_FRONT_PORCH,
            vertical_blank_front_porch: VSYNC_PULSE + VSYNC_BACK_PORCH,

            hsync_width: HSYNC_PULSE,
            vsync_width: VSYNC_PULSE,

            hsync_position: 0,
        })
        .with_vsync_idle_level(Level::High)
        .with_hsync_idle_level(Level::High)
        .with_de_idle_level(Level::Low)
        .with_disable_black_region(false)
}
