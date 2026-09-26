//! Astronaut board (ESP32-P4-MODULE-32MB), from `Electronic/Astronaut`.
//!
//! Display (same 320x960 panel as the Qualia), on the ESP32-P4:
//! - R0..R5: GPIO37, 36, 34, 33, 32, 27
//! - G0..G5: GPIO43, 42, 41, 40, 39, 38
//! - B0..B5: GPIO49, 48, 47, 46, 45, 44
//! - HSYNC GPIO53, VSYNC GPIO50, DE GPIO51, PCLK GPIO54 (through R18, 100R)
//! - Panel reset (RST): GPIO28, backlight: GPIO26
//! - Panel SPI: SCK GPIO30, MOSI GPIO31 (shared with the Ethernet chip), TFT_CS GPIO20
//! - TP_IRQ GPIO52, I2C SDA GPIO7 / SCL GPIO8
//!
//! Buttons: BUTTON1 = GPIO35 (BUTTON2 is ESP_EN).
//!
//! Not driven yet: esp-hal 1.2 has no RGB display driver for the ESP32-P4. The framebuffer
//! and UI still run; only the scan-out is missing.

