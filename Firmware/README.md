# Firmware

## Setup

### Toolchain

Follow the official Espressif Rust toolchain guide:
https://docs.espressif.com/projects/rust/book/getting-started/toolchain.html

### Targets

| Chip     | Board                | Usage            |
| -------- | -------------------- | ---------------- |
| ESP32-S3 | Adafruit Qualia S3   | Development      |
| ESP32-P4 | Astronaut board      | Astronaut system |

- **ESP32-S3** uses an Xtensa core: it needs the Espressif Rust fork, installed with `espup`.
- **ESP32-P4** uses a RISC-V core: see the RISC-V section of the guide above.

## Building

The `astronaut` firmware builds for either board. Run from `Firmware/astronaut/`:

| Command                  | Board     | Action                    |
| ------------------------ | --------- | ------------------------- |
| `cargo qualia`           | Qualia S3 | Build, flash and monitor  |
| `cargo astronaut`        | Astronaut | Build, flash and monitor  |
| `cargo build-qualia`     | Qualia S3 | Build only                |
| `cargo build-astronaut`  | Astronaut | Build only                |

Extra arguments are passed through, e.g. `cargo qualia --release`.
A plain `cargo build` / `cargo run` targets the Qualia.

Board-specific code goes behind `#[cfg(feature = "qualia")]` / `#[cfg(feature = "astronaut")]`.

## Qualia

Adafruit Qualia S3, used as the development board.

- Module: ESP32-S3-WROOM-1
- Flash: 16 MB
- PSRAM: 8 MB octal

## Display

Both boards use the Adafruit 4.58" 320x960 RGB bar display (product 5805, ST7701S) with a
[Slint](https://slint.dev) UI (`astronaut/ui/app.slint`). Shared code is in
`astronaut/src/display/`, board wiring in `astronaut/src/board/`.

- Qualia: the panel is configured over SPI bit-banged through the I2C expander, then scanned
  out by the S3 RGB peripheral through internal-RAM bounce buffers (direct PSRAM DMA drifts).
- Astronaut: not scanned out yet, esp-hal has no RGB display driver for the ESP32-P4.
