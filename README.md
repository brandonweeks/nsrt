# NSRT

A Rust driver for the NSRT_mk4 sound level meter from Convergence Instruments.

## Features

- Read sound pressure levels (SPL) in dB
- Read LEQ (Equivalent Continuous Sound Level)
- Configure weighting curves (A, C, Z)
- Set sampling frequency and time constants
- Read device information and temperature
- Fluent API for device configuration

## Usage

```rust
use nsrt::{NSRT, Weighting, SamplingFrequency};

fn main() -> nsrt::Result<()> {
    // Connect to the device and configure it in a single chain
    let mut nsrt = NSRT::open()?
        .weighting(Weighting::A)?
        .time_constant(1.0)?
        .sampling_frequency(SamplingFrequency::Freq48kHz)?
        .apply()?;

    // Read measurements
    let level = nsrt.read_level()?;
    println!("Current sound level: {:.1} dBA", level);

    Ok(())
}
```

See `examples/simple_monitor.rs` for a more complete example.
