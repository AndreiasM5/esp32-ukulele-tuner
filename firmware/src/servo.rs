use anyhow::Result;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::ledc::LedcDriver;

const SERVO_PERIOD_US: u32 = 20_000;
const STOP_US: u32 = 1_500;
const MIN_MOVE_DELTA_US: u32 = 320;
const MAX_MOVE_DELTA_US: u32 = 500;
const MIN_RUN_MS: u32 = 320;
const MAX_RUN_MS: u32 = 900;

pub const TIGHTEN_IS_CLOCKWISE: bool = false;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServoDirection {
    Tighten,
    Loosen,
}

#[derive(Clone, Copy, Debug)]
pub struct ServoMove {
    pub pulse_us: u32,
    pub run_ms: u32,
}

pub fn stop(servo: &mut LedcDriver<'_>) -> Result<()> {
    set_pulse_us(servo, STOP_US)
}

pub fn self_test(servo: &mut LedcDriver<'_>) -> Result<()> {
    set_pulse_us(servo, 2_000)?;
    FreeRtos::delay_ms(550);
    stop(servo)?;
    FreeRtos::delay_ms(250);
    set_pulse_us(servo, 1_000)?;
    FreeRtos::delay_ms(550);
    stop(servo)?;
    Ok(())
}

pub fn plan(direction: ServoDirection, cents_error: f32) -> ServoMove {
    let cents = cents_error.abs().clamp(0.0, 120.0);
    let speed = MIN_MOVE_DELTA_US
        + ((MAX_MOVE_DELTA_US - MIN_MOVE_DELTA_US) as f32 * (cents / 120.0)) as u32;
    let run_ms = (MIN_RUN_MS as f32 + cents * 4.0) as u32;
    let run_ms = run_ms.clamp(MIN_RUN_MS, MAX_RUN_MS);

    let clockwise = match direction {
        ServoDirection::Tighten => TIGHTEN_IS_CLOCKWISE,
        ServoDirection::Loosen => !TIGHTEN_IS_CLOCKWISE,
    };

    let pulse_us = if clockwise {
        STOP_US + speed
    } else {
        STOP_US - speed
    };

    ServoMove { pulse_us, run_ms }
}

pub fn nudge(
    servo: &mut LedcDriver<'_>,
    direction: ServoDirection,
    cents_error: f32,
) -> Result<u32> {
    let servo_move = plan(direction, cents_error);

    set_pulse_us(servo, servo_move.pulse_us)?;
    FreeRtos::delay_ms(servo_move.run_ms);
    stop(servo)?;

    Ok(servo_move.run_ms)
}

fn set_pulse_us(servo: &mut LedcDriver<'_>, pulse_us: u32) -> Result<()> {
    let max_duty = servo.get_max_duty();
    servo.set_duty(max_duty * pulse_us / SERVO_PERIOD_US)?;
    Ok(())
}
