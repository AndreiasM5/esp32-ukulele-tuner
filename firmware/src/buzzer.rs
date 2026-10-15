use anyhow::Result;
use esp_idf_svc::hal::delay::FreeRtos;
use esp_idf_svc::hal::ledc::LedcDriver;

pub const BUZZER_FREQ_HZ: u32 = 2_200;

pub fn silence(buzzer: &mut LedcDriver<'_>) -> Result<()> {
    buzzer.set_duty(0)?;
    Ok(())
}

pub fn ok_beep(buzzer: &mut LedcDriver<'_>) -> Result<()> {
    let max_duty = buzzer.get_max_duty();

    buzzer.set_duty(max_duty / 3)?;
    FreeRtos::delay_ms(90);
    buzzer.set_duty(0)?;
    FreeRtos::delay_ms(70);
    buzzer.set_duty(max_duty / 3)?;
    FreeRtos::delay_ms(90);
    buzzer.set_duty(0)?;

    Ok(())
}
