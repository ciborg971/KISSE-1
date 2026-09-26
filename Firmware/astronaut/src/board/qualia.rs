//! Adafruit Qualia S3 (ESP32-S3-WROOM-1, 16 MB flash, 8 MB octal PSRAM).
//!
//! - I2C: SDA = GPIO8, SCL = GPIO18
//! - RGB bus: see `run_qualia` in `main.rs`
//! - GPIO expander (PCA9554A-compatible) at 0x3F:
//!   0 TFT_SCK, 1 TFT_CS, 2 TFT_RESET, 3 TP_IRQ, 4 BACKLIGHT, 5 BTN_UP, 6 BTN_DN, 7 TFT_MOSI

use esp_hal::{
    Blocking,
    i2c::master::{Error, I2c},
};

use crate::display::panel::{PanelBus, PanelPins};

/// I2C address of the GPIO expander.
pub const EXPANDER_ADDR: u8 = 0x3F;

const REG_OUTPUT: u8 = 0x01;
const REG_POLARITY: u8 = 0x02;
const REG_CONFIG: u8 = 0x03;

const TFT_SCK: u8 = 1 << 0;
const TFT_CS: u8 = 1 << 1;
const TFT_RESET: u8 = 1 << 2;
const TFT_MOSI: u8 = 1 << 7;
/// Direction register (1 = input): TP_IRQ, BACKLIGHT, BTN_UP, BTN_DN.
const INPUTS: u8 = 0x78;

/// Panel configuration lines driven through the expander output register.
pub struct ExpanderPanelBus<'a, 'd> {
    i2c: &'a mut I2c<'d, Blocking>,
}

impl<'a, 'd> ExpanderPanelBus<'a, 'd> {
    /// Configure the expander pin directions for driving the panel.
    pub fn new(i2c: &'a mut I2c<'d, Blocking>) -> Result<Self, Error> {
        i2c.write(EXPANDER_ADDR, &[REG_CONFIG, INPUTS])?;
        i2c.write(EXPANDER_ADDR, &[REG_POLARITY, 0])?;
        Ok(Self { i2c })
    }
}

impl PanelBus for ExpanderPanelBus<'_, '_> {
    type Error = Error;

    fn write(&mut self, pins: PanelPins) -> Result<(), Error> {
        let mut out = INPUTS;
        for (on, bit) in [
            (pins.sck, TFT_SCK),
            (pins.cs, TFT_CS),
            (pins.reset, TFT_RESET),
            (pins.mosi, TFT_MOSI),
        ] {
            if on {
                out |= bit;
            }
        }
        self.i2c.write(EXPANDER_ADDR, &[REG_OUTPUT, out])
    }
}
