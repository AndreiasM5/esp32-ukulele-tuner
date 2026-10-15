use anyhow::Result;
use embedded_hal::spi::{Mode, Phase, Polarity};
use esp_idf_svc::hal::delay::{FreeRtos, TickType};
use esp_idf_svc::hal::gpio::PinDriver;
use esp_idf_svc::hal::i2s::config::{
    Config as I2sConfig, DataBitWidth, SlotBitWidth, SlotMode, StdClkConfig, StdConfig,
    StdGpioConfig, StdSlotConfig, StdSlotMask,
};
use esp_idf_svc::hal::i2s::{I2sDriver, I2sRx};
use esp_idf_svc::hal::ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver};
use esp_idf_svc::hal::peripherals::Peripherals;
use esp_idf_svc::hal::spi::{config, SpiDeviceDriver};
use esp_idf_svc::hal::units::FromValueType;

mod audio;
mod buzzer;
mod display;
mod servo;
mod tuner;

use audio::{decode_inmp441_samples, level, SAMPLE_COUNT, SAMPLE_RATE_HZ};
use display::St7735;
use servo::ServoDirection;
use tuner::{detect_pitch, PitchReading, TuneState};

const STABLE_READS_BEFORE_ACTION: u8 = 1;
const READ_TIMEOUT_MS: u32 = 300;
const SERVO_NOISE_IGNORE_MS: u32 = 420;
const DRAIN_READS: usize = 5;

fn main() -> Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;

    let spi_config = config::Config::new()
        .baudrate(20.MHz().into())
        .data_mode(Mode {
            polarity: Polarity::IdleLow,
            phase: Phase::CaptureOnFirstTransition,
        });

    let lcd_spi = SpiDeviceDriver::new_single(
        peripherals.spi2,
        peripherals.pins.gpio18,
        peripherals.pins.gpio23,
        Option::<esp_idf_svc::hal::gpio::AnyIOPin>::None,
        Option::<esp_idf_svc::hal::gpio::AnyOutputPin>::None,
        &config::DriverConfig::new(),
        &spi_config,
    )?;
    let lcd_dc = PinDriver::output(peripherals.pins.gpio17)?;
    let lcd_rst = PinDriver::output(peripherals.pins.gpio16)?;
    let mut lcd = St7735::new(lcd_spi, lcd_dc, lcd_rst);
    lcd.init()?;
    lcd.draw_boot()?;

    let i2s_config = StdConfig::new(
        I2sConfig::new().dma_buffer_count(6).frames_per_buffer(256),
        StdClkConfig::from_sample_rate_hz(SAMPLE_RATE_HZ),
        StdSlotConfig::philips_slot_default(DataBitWidth::Bits32, SlotMode::Mono)
            .slot_bit_width(SlotBitWidth::Bits32)
            .slot_mode_mask(SlotMode::Mono, StdSlotMask::Left),
        StdGpioConfig::default(),
    );
    let mut mic = I2sDriver::<I2sRx>::new_std_rx(
        peripherals.i2s0,
        &i2s_config,
        peripherals.pins.gpio26,
        peripherals.pins.gpio33,
        Option::<esp_idf_svc::hal::gpio::AnyIOPin>::None,
        peripherals.pins.gpio25,
    )?;
    mic.rx_enable()?;

    let mut servo = LedcDriver::new(
        peripherals.ledc.channel0,
        LedcTimerDriver::new(
            peripherals.ledc.timer0,
            &TimerConfig::new().frequency(50.Hz().into()),
        )?,
        peripherals.pins.gpio13,
    )?;
    servo::stop(&mut servo)?;
    lcd.show_wait("TEST SERVO")?;
    servo::self_test(&mut servo)?;

    let mut buzzer = LedcDriver::new(
        peripherals.ledc.channel1,
        LedcTimerDriver::new(
            peripherals.ledc.timer1,
            &TimerConfig::new().frequency(buzzer::BUZZER_FREQ_HZ.Hz().into()),
        )?,
        peripherals.pins.gpio14,
    )?;
    buzzer::silence(&mut buzzer)?;

    FreeRtos::delay_ms(600);
    lcd.show_listening()?;

    let mut audio_bytes = vec![0u8; SAMPLE_COUNT * 4];
    let mut samples = vec![0.0f32; SAMPLE_COUNT];
    let mut waiting_for_silence = false;
    let mut listening_screen_visible = true;
    let mut stable_reading: Option<PitchReading> = None;
    let mut stable_count = 0u8;

    loop {
        let bytes_read = mic.read(
            &mut audio_bytes,
            TickType::new_millis(READ_TIMEOUT_MS.into()).ticks(),
        )?;

        if bytes_read < audio_bytes.len() / 4 {
            if !listening_screen_visible {
                lcd.show_wait("I2S WAIT")?;
                listening_screen_visible = true;
            }
            continue;
        }

        let count = decode_inmp441_samples(&audio_bytes[..bytes_read], &mut samples);
        let active_samples = &samples[..count];
        let audio_level = level(active_samples);

        if waiting_for_silence {
            if audio_level.rms <= audio::QUIET_RMS {
                waiting_for_silence = false;
                stable_reading = None;
                stable_count = 0;
                lcd.show_listening()?;
                listening_screen_visible = true;
            }
            FreeRtos::delay_ms(90);
            continue;
        }

        let Some(reading) = detect_pitch(active_samples, audio_level.rms, audio_level.peak) else {
            stable_reading = None;
            stable_count = 0;
            if !listening_screen_visible {
                lcd.show_listening()?;
                listening_screen_visible = true;
            }
            FreeRtos::delay_ms(90);
            continue;
        };

        if stable_reading
            .map(|previous| reading.same_bucket_as(previous))
            .unwrap_or(false)
        {
            stable_count = stable_count.saturating_add(1);
        } else {
            stable_count = 1;
        }
        stable_reading = Some(reading);
        listening_screen_visible = false;

        lcd.show_reading(&reading, stable_count)?;
        log::info!(
            "{} {:.1}Hz {:+.1}c conf {:.2} rms {:.4}",
            reading.target.name,
            reading.freq_hz,
            reading.cents,
            reading.confidence,
            audio_level.rms
        );

        if stable_count < STABLE_READS_BEFORE_ACTION {
            FreeRtos::delay_ms(120);
            continue;
        }

        match reading.state {
            TuneState::Flat => {
                nudge_and_wait(
                    &mut lcd,
                    &mut mic,
                    &mut audio_bytes,
                    &mut servo,
                    ServoDirection::Tighten,
                    reading.cents,
                )?;
                waiting_for_silence = true;
            }
            TuneState::Sharp => {
                nudge_and_wait(
                    &mut lcd,
                    &mut mic,
                    &mut audio_bytes,
                    &mut servo,
                    ServoDirection::Loosen,
                    reading.cents,
                )?;
                waiting_for_silence = true;
            }
            TuneState::InTune => {
                servo::stop(&mut servo)?;
                lcd.show_ok(&reading)?;
                buzzer::ok_beep(&mut buzzer)?;
                drain_mic(&mut mic, &mut audio_bytes)?;
                waiting_for_silence = true;
            }
        }

        stable_reading = None;
        stable_count = 0;
    }
}

fn nudge_and_wait<SPI, DC, RST>(
    lcd: &mut St7735<SPI, DC, RST>,
    mic: &mut I2sDriver<'_, I2sRx>,
    audio_bytes: &mut [u8],
    servo: &mut LedcDriver<'_>,
    direction: ServoDirection,
    cents: f32,
) -> Result<()>
where
    SPI: embedded_hal::spi::SpiDevice,
    DC: embedded_hal::digital::OutputPin,
    RST: embedded_hal::digital::OutputPin,
{
    let servo_move = servo::plan(direction, cents);
    lcd.show_action(direction, servo_move.run_ms)?;
    let run_ms = servo::nudge(servo, direction, cents)?;
    lcd.show_action(direction, run_ms)?;
    FreeRtos::delay_ms(SERVO_NOISE_IGNORE_MS);
    drain_mic(mic, audio_bytes)?;
    lcd.show_wait("DIN NOU")?;
    Ok(())
}

fn drain_mic(mic: &mut I2sDriver<'_, I2sRx>, audio_bytes: &mut [u8]) -> Result<()> {
    for _ in 0..DRAIN_READS {
        let _ = mic.read(audio_bytes, TickType::new_millis(40).ticks())?;
    }
    Ok(())
}
