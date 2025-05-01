use nsrt::{NSRT, Result, Weighting};
use std::thread;
use std::time::Duration;

fn main() -> Result<()> {
    // Open the first available NSRT device
    println!("Opening NSRT_mk4 device...");
    let mut nsrt = NSRT::open()?;

    // Read device information
    let model = nsrt.read_model()?;
    let serial = nsrt.read_serial_number()?;
    let firmware = nsrt.read_firmware_revision()?;

    println!("Connected to:");
    println!("  Model: {}", model);
    println!("  Serial: {}", serial);
    println!("  Firmware: {}", firmware);

    // Configure the device
    println!("Configuring device...");
    nsrt.write_weighting(Weighting::A)?;
    nsrt.write_time_constant(1.0)?; // 1 second time constant

    // Wait for stabilization after changing parameters
    println!("Waiting for stabilization...");
    nsrt.wait_for_stabilization(1.0);

    // Monitor sound levels for 10 seconds
    println!("Monitoring sound levels for 10 seconds:");
    println!("Time (s) | Level (dBA) | LEQ (dBA) | Temp (°C)");
    println!("---------+------------+-----------+----------");

    loop {
        let level = nsrt.read_level()?;
        let leq = nsrt.read_leq()?;
        let temp = nsrt.read_temperature()?;

        println!("{:10.1} | {:9.1} | {:8.1}", level, leq, temp);

        thread::sleep(Duration::from_secs(1));
    }
}
