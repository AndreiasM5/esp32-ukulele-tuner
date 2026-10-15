use anyhow::{anyhow, Result};
use embedded_hal::digital::OutputPin;
use embedded_hal::spi::SpiDevice;
use esp_idf_svc::hal::delay::FreeRtos;

use crate::servo::ServoDirection;
use crate::tuner::{PitchReading, TuneState, OK_TOLERANCE_CENTS};

const LCD_W: u16 = 128;
const LCD_H: u16 = 128;
const LCD_X_OFFSET: u16 = 4;
const LCD_Y_OFFSET: u16 = 3;

const BG: u16 = 0x0000;
const TEXT: u16 = 0xffff;
const MUTED: u16 = 0x8410;
const RED: u16 = 0xc904;
const GREEN: u16 = 0x05c8;
const CYAN: u16 = 0x04bf;
const YELLOW: u16 = 0xd540;
const BLUE: u16 = 0x02bf;

pub struct St7735<SPI, DC, RST> {
    spi: SPI,
    dc: DC,
    rst: RST,
}

impl<SPI, DC, RST> St7735<SPI, DC, RST>
where
    SPI: SpiDevice,
    DC: OutputPin,
    RST: OutputPin,
{
    pub fn new(spi: SPI, dc: DC, rst: RST) -> Self {
        Self { spi, dc, rst }
    }

    pub fn init(&mut self) -> Result<()> {
        pin_high(&mut self.rst)?;
        FreeRtos::delay_ms(20);
        pin_low(&mut self.rst)?;
        FreeRtos::delay_ms(20);
        pin_high(&mut self.rst)?;
        FreeRtos::delay_ms(150);

        self.command(0x01)?;
        FreeRtos::delay_ms(150);
        self.command(0x11)?;
        FreeRtos::delay_ms(120);
        self.command_data(0x3a, &[0x05])?;
        self.command_data(0x36, &[0xa8])?;
        self.command(0x20)?;
        self.command(0x29)?;
        FreeRtos::delay_ms(100);
        self.clear(BG)?;

        Ok(())
    }

    pub fn draw_boot(&mut self) -> Result<()> {
        self.clear(BG)?;
        self.draw_text(8, 8, "UKULELE", 2, CYAN)?;
        self.draw_text(23, 30, "TUNER", 2, TEXT)?;
        self.draw_text(10, 62, "GCEA STANDARD", 1, YELLOW)?;
        self.draw_text(13, 88, "PORNESC...", 1, MUTED)?;
        Ok(())
    }

    pub fn show_listening(&mut self) -> Result<()> {
        self.clear_status_area()?;
        self.draw_text(14, 58, "CIUPESTE", 2, YELLOW)?;
        self.draw_text(15, 83, "O COARDA", 2, TEXT)?;
        self.draw_text(8, 112, "G4 C4 E4 A4", 1, MUTED)?;
        Ok(())
    }

    pub fn show_wait(&mut self, message: &str) -> Result<()> {
        self.clear_status_area()?;
        self.draw_text(10, 63, message, 2, YELLOW)?;
        Ok(())
    }

    pub fn show_reading(&mut self, reading: &PitchReading, stable_count: u8) -> Result<()> {
        self.clear_status_area()?;

        let state_text = match reading.state {
            TuneState::Flat => "STRANGE",
            TuneState::Sharp => "SLABESTE",
            TuneState::InTune => "OK",
        };
        let state_color = match reading.state {
            TuneState::Flat => YELLOW,
            TuneState::Sharp => RED,
            TuneState::InTune => GREEN,
        };

        self.draw_text(7, 53, reading.target.name, 3, TEXT)?;
        self.draw_text(55, 55, &format!("{:.1}HZ", reading.freq_hz), 1, CYAN)?;
        self.draw_text(55, 68, &format!("{:+.0}C", reading.cents), 1, TEXT)?;
        self.draw_text(7, 84, state_text, 2, state_color)?;
        self.draw_meter(reading.cents)?;

        let status = if stable_count >= 2 {
            "CONFIRMAT"
        } else {
            "MASOR"
        };
        self.draw_text(69, 106, status, 1, MUTED)?;

        Ok(())
    }

    pub fn show_action(&mut self, direction: ServoDirection, run_ms: u32) -> Result<()> {
        self.fill_rect(0, 104, LCD_W, 24, BG)?;
        let message = match direction {
            ServoDirection::Tighten => "SERVO +",
            ServoDirection::Loosen => "SERVO -",
        };
        self.draw_text(7, 108, message, 1, YELLOW)?;
        self.draw_text(70, 108, &format!("{}MS", run_ms), 1, MUTED)?;
        Ok(())
    }

    pub fn show_ok(&mut self, reading: &PitchReading) -> Result<()> {
        self.show_reading(reading, 2)?;
        self.fill_rect(60, 82, 66, 20, BG)?;
        self.draw_text(64, 86, "GATA", 2, GREEN)?;
        Ok(())
    }

    fn draw_meter(&mut self, cents: f32) -> Result<()> {
        let y = 119;
        self.fill_rect(7, y, 114, 5, MUTED)?;
        self.fill_rect(62, y - 4, 4, 13, TEXT)?;

        let clamped = cents.clamp(-60.0, 60.0);
        let offset = (clamped / 60.0 * 54.0) as i16;
        let x = (64i16 + offset - 3).clamp(7, 115) as u16;
        let color = if cents.abs() <= OK_TOLERANCE_CENTS {
            GREEN
        } else if cents < 0.0 {
            YELLOW
        } else {
            RED
        };
        self.fill_rect(x, y - 3, 7, 11, color)?;
        self.fill_rect(61, y - 8, 6, 3, BLUE)?;
        Ok(())
    }

    fn clear_status_area(&mut self) -> Result<()> {
        self.fill_rect(0, 50, LCD_W, 78, BG)
    }

    fn command(&mut self, command: u8) -> Result<()> {
        pin_low(&mut self.dc)?;
        spi_write(&mut self.spi, &[command])
    }

    fn data(&mut self, data: &[u8]) -> Result<()> {
        pin_high(&mut self.dc)?;
        spi_write(&mut self.spi, data)
    }

    fn command_data(&mut self, command: u8, data: &[u8]) -> Result<()> {
        self.command(command)?;
        self.data(data)
    }

    fn set_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) -> Result<()> {
        let x0 = x0 + LCD_X_OFFSET;
        let x1 = x1 + LCD_X_OFFSET;
        let y0 = y0 + LCD_Y_OFFSET;
        let y1 = y1 + LCD_Y_OFFSET;

        self.command_data(
            0x2a,
            &[(x0 >> 8) as u8, x0 as u8, (x1 >> 8) as u8, x1 as u8],
        )?;
        self.command_data(
            0x2b,
            &[(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8],
        )?;
        self.command(0x2c)
    }

    fn raw_set_window(&mut self, x0: u16, y0: u16, x1: u16, y1: u16) -> Result<()> {
        self.command_data(
            0x2a,
            &[(x0 >> 8) as u8, x0 as u8, (x1 >> 8) as u8, x1 as u8],
        )?;
        self.command_data(
            0x2b,
            &[(y0 >> 8) as u8, y0 as u8, (y1 >> 8) as u8, y1 as u8],
        )?;
        self.command(0x2c)
    }

    fn clear(&mut self, color: u16) -> Result<()> {
        self.raw_fill_rect(0, 0, 132, 132, color)?;
        self.fill_rect(0, 0, LCD_W, LCD_H, color)
    }

    fn raw_fill_rect(&mut self, x: u16, y: u16, w: u16, h: u16, color: u16) -> Result<()> {
        if w == 0 || h == 0 {
            return Ok(());
        }

        self.raw_set_window(x, y, x + w - 1, y + h - 1)?;
        self.fill_pixels(w as usize * h as usize, color)
    }

    fn fill_rect(&mut self, x: u16, y: u16, w: u16, h: u16, color: u16) -> Result<()> {
        if w == 0 || h == 0 || x >= LCD_W || y >= LCD_H {
            return Ok(());
        }

        let w = w.min(LCD_W - x);
        let h = h.min(LCD_H - y);
        self.set_window(x, y, x + w - 1, y + h - 1)?;
        self.fill_pixels(w as usize * h as usize, color)
    }

    fn fill_pixels(&mut self, pixels: usize, color: u16) -> Result<()> {
        let hi = (color >> 8) as u8;
        let lo = color as u8;
        let mut chunk = [0u8; 256];
        for px in chunk.chunks_exact_mut(2) {
            px[0] = hi;
            px[1] = lo;
        }

        let mut remaining = pixels;
        while remaining > 0 {
            let pixels = remaining.min(chunk.len() / 2);
            self.data(&chunk[..pixels * 2])?;
            remaining -= pixels;
        }

        Ok(())
    }

    fn draw_text(&mut self, x: u16, y: u16, text: &str, scale: u16, color: u16) -> Result<()> {
        let mut cursor_x = x;

        for ch in text.chars() {
            if ch == ' ' {
                cursor_x += 4 * scale;
                continue;
            }

            self.draw_char(cursor_x, y, ch, scale, color)?;
            cursor_x += 6 * scale;
        }

        Ok(())
    }

    fn draw_char(&mut self, x: u16, y: u16, ch: char, scale: u16, color: u16) -> Result<()> {
        let glyph = glyph_5x7(ch);

        for (col, bits) in glyph.iter().enumerate() {
            for row in 0..7 {
                if bits & (1 << row) != 0 {
                    self.fill_rect(
                        x + col as u16 * scale,
                        y + row as u16 * scale,
                        scale,
                        scale,
                        color,
                    )?;
                }
            }
        }

        Ok(())
    }
}

fn pin_high<P>(pin: &mut P) -> Result<()>
where
    P: OutputPin,
{
    pin.set_high().map_err(|_| anyhow!("gpio high failed"))
}

fn pin_low<P>(pin: &mut P) -> Result<()>
where
    P: OutputPin,
{
    pin.set_low().map_err(|_| anyhow!("gpio low failed"))
}

fn spi_write<SPI>(spi: &mut SPI, data: &[u8]) -> Result<()>
where
    SPI: SpiDevice,
{
    spi.write(data).map_err(|_| anyhow!("spi write failed"))
}

fn glyph_5x7(ch: char) -> [u8; 5] {
    match ch.to_ascii_uppercase() {
        'A' => [0x7e, 0x11, 0x11, 0x11, 0x7e],
        'B' => [0x7f, 0x49, 0x49, 0x49, 0x36],
        'C' => [0x3e, 0x41, 0x41, 0x41, 0x22],
        'D' => [0x7f, 0x41, 0x41, 0x22, 0x1c],
        'E' => [0x7f, 0x49, 0x49, 0x49, 0x41],
        'F' => [0x7f, 0x09, 0x09, 0x09, 0x01],
        'G' => [0x3e, 0x41, 0x49, 0x49, 0x7a],
        'H' => [0x7f, 0x08, 0x08, 0x08, 0x7f],
        'I' => [0x00, 0x41, 0x7f, 0x41, 0x00],
        'J' => [0x20, 0x40, 0x41, 0x3f, 0x01],
        'K' => [0x7f, 0x08, 0x14, 0x22, 0x41],
        'L' => [0x7f, 0x40, 0x40, 0x40, 0x40],
        'M' => [0x7f, 0x02, 0x0c, 0x02, 0x7f],
        'N' => [0x7f, 0x04, 0x08, 0x10, 0x7f],
        'O' => [0x3e, 0x41, 0x41, 0x41, 0x3e],
        'P' => [0x7f, 0x09, 0x09, 0x09, 0x06],
        'Q' => [0x3e, 0x41, 0x51, 0x21, 0x5e],
        'R' => [0x7f, 0x09, 0x19, 0x29, 0x46],
        'S' => [0x46, 0x49, 0x49, 0x49, 0x31],
        'T' => [0x01, 0x01, 0x7f, 0x01, 0x01],
        'U' => [0x3f, 0x40, 0x40, 0x40, 0x3f],
        'V' => [0x1f, 0x20, 0x40, 0x20, 0x1f],
        'W' => [0x7f, 0x20, 0x18, 0x20, 0x7f],
        'X' => [0x63, 0x14, 0x08, 0x14, 0x63],
        'Y' => [0x07, 0x08, 0x70, 0x08, 0x07],
        'Z' => [0x61, 0x51, 0x49, 0x45, 0x43],
        '0' => [0x3e, 0x51, 0x49, 0x45, 0x3e],
        '1' => [0x00, 0x42, 0x7f, 0x40, 0x00],
        '2' => [0x42, 0x61, 0x51, 0x49, 0x46],
        '3' => [0x21, 0x41, 0x45, 0x4b, 0x31],
        '4' => [0x18, 0x14, 0x12, 0x7f, 0x10],
        '5' => [0x27, 0x45, 0x45, 0x45, 0x39],
        '6' => [0x3c, 0x4a, 0x49, 0x49, 0x30],
        '7' => [0x01, 0x71, 0x09, 0x05, 0x03],
        '8' => [0x36, 0x49, 0x49, 0x49, 0x36],
        '9' => [0x06, 0x49, 0x49, 0x29, 0x1e],
        '+' => [0x08, 0x08, 0x3e, 0x08, 0x08],
        '-' => [0x08, 0x08, 0x08, 0x08, 0x08],
        '.' => [0x00, 0x60, 0x60, 0x00, 0x00],
        '/' => [0x20, 0x10, 0x08, 0x04, 0x02],
        ':' => [0x00, 0x36, 0x36, 0x00, 0x00],
        _ => [0x00, 0x00, 0x00, 0x00, 0x00],
    }
}
