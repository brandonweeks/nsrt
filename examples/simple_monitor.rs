use nsrt::{NSRT, Result, SamplingFrequency, Weighting};
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    println!("Opening NSRT_mk4 device...");

    let mut nsrt = NSRT::open()?
        .weighting(Weighting::A)?
        .time_constant(1.0)?
        .sampling_frequency(SamplingFrequency::Freq48kHz)?
        .apply()?;

    let model = nsrt.read_model()?;
    let serial = nsrt.read_serial_number()?;
    let firmware = nsrt.read_firmware_revision()?;

    println!("Connected to:");
    println!("  Model: {}", model);
    println!("  Serial: {}", serial);
    println!("  Firmware: {}", firmware);

    println!("Monitoring sound levels:");
    println!("Level (dBA) | LEQ (dBA) | Temp (°C)");
    println!("------------+-----------+----------");

    loop {
        let level = nsrt.read_level()?;
        let leq = nsrt.read_leq()?;
        let temp = nsrt.read_temperature()?;

        println!("{:10.1} | {:9.1} | {:8.1}", level, leq, temp);

        thread::sleep(Duration::from_secs(1));
    }
}
