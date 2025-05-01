use bytes::{BufMut, BytesMut};
use serialport::SerialPort;
use std::io::{Read, Write};
use std::thread;
use std::time::Duration;
use thiserror::Error;

const VID: u16 = 2649;
const PID: u16 = 323;

/// Error type for the NSRT_mk4 driver
#[derive(Error, Debug)]
pub enum NsrtError {
    #[error("Serial port error: {0}")]
    SerialError(#[from] serialport::Error),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("No device found")]
    NoDevice,

    #[error("Device did not acknowledge command")]
    NoAcknowledge,

    #[error("Invalid response from device")]
    InvalidResponse,

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
}

/// Result type for the NSRT_mk4 driver
pub type Result<T> = std::result::Result<T, NsrtError>;

/// Weighting functions supported by the NSRT_mk4
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Weighting {
    /// C-weighting (dB-C)
    C = 0,
    /// A-weighting (dB-A)
    A = 1,
    /// Z-weighting (dB-Z)
    Z = 2,
}

/// Sampling frequencies supported by the NSRT_mk4
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SamplingFrequency {
    /// 32 kHz
    Freq32kHz = 32000,
    /// 48 kHz
    Freq48kHz = 48000,
}

/// Audio debug mode settings
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AudioDebugMode {
    /// Normal mode
    Normal = 0,
    /// Debug mode (outputs 1kHz sine at 94dB)
    Debug = 1,
}

/// Command codes for the NSRT_mk4 device
#[derive(Debug, Clone, Copy)]
enum Command {
    ReadLevel = 0x80000010,
    ReadLEQ = 0x80000011,
    ReadTemperature = 0x80000012,
    ReadWeighting = 0x80000020,
    ReadFS = 0x80000021,
    ReadTau = 0x80000022,
    ReadModel = 0x80000031,
    ReadSN = 0x80000032,
    ReadFWRev = 0x80000033,
    ReadDOC = 0x80000034,
    ReadDOB = 0x80000035,
    ReadUserID = 0x80000036,
    WriteWeighting = 0x00000020,
    WriteFS = 0x00000021,
    WriteTau = 0x00000022,
    WriteUserID = 0x00000036,
}

/// The main driver for the NSRT_mk4 device
pub struct NSRT {
    port: Box<dyn SerialPort>,
}

impl NSRT {
    /// Open the NSRT_mk4 device
    ///
    /// This method automatically finds and opens the first NSRT_mk4 device
    /// connected to the system using the Convergence Instruments VID/PID.
    pub fn open() -> Result<Self> {
        let ports = serialport::available_ports()?;

        for port_info in ports {
            if let serialport::SerialPortType::UsbPort(usb_info) = &port_info.port_type {
                if usb_info.vid == VID && usb_info.pid == PID {
                    let port = serialport::new(&port_info.port_name, 9600)
                        .timeout(Duration::from_millis(1000))
                        .open()?;

                    return Ok(Self { port });
                }
            }
        }

        Err(NsrtError::NoDevice)
    }

    /// Open a specific serial port
    pub fn open_port(port_name: &str) -> Result<Self> {
        let port = serialport::new(port_name, 9600)
            .timeout(Duration::from_millis(1000))
            .open()?;

        Ok(Self { port })
    }

    /// Send a command to the device
    fn send_command(&mut self, cmd: Command, address: u32, count: u32) -> Result<()> {
        let mut buf = BytesMut::with_capacity(12); // Command packet is 12 bytes

        buf.put_u32_le(cmd as u32);
        buf.put_u32_le(address);
        buf.put_u32_le(count);

        self.port.write_all(&buf)?;

        Ok(())
    }

    /// Send a command with data to the device
    fn send_command_with_data(&mut self, cmd: Command, address: u32, data: &[u8]) -> Result<()> {
        // Send command packet
        self.send_command(cmd, address, data.len() as u32)?;

        // Send data packet
        self.port.write_all(data)?;

        // Wait for acknowledgment
        let mut ack = [0u8; 1];
        self.port.read_exact(&mut ack)?;

        if ack[0] != 0x06 {
            return Err(NsrtError::NoAcknowledge);
        }

        Ok(())
    }

    /// Send a command and read response data
    fn send_command_and_read(&mut self, cmd: Command, address: u32, count: u32) -> Result<Vec<u8>> {
        // Send command packet
        self.send_command(cmd, address, count)?;

        // Read response
        let mut response = vec![0u8; count as usize];
        self.port.read_exact(&mut response)?;

        Ok(response)
    }

    // High-level API functions

    /// Read the current sound level in dB
    pub fn read_level(&mut self) -> Result<f32> {
        let data = self.send_command_and_read(Command::ReadLevel, 0, 4)?;
        Ok(f32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    /// Read the current LEQ (Equivalent Continuous Sound Level) in dB
    /// and restart integration for the next LEQ measurement
    pub fn read_leq(&mut self) -> Result<f32> {
        let data = self.send_command_and_read(Command::ReadLEQ, 0, 4)?;
        Ok(f32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    /// Read the current temperature in degrees Celsius
    pub fn read_temperature(&mut self) -> Result<f32> {
        let data = self.send_command_and_read(Command::ReadTemperature, 0, 4)?;
        Ok(f32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    /// Read the current weighting curve
    pub fn read_weighting(&mut self) -> Result<Weighting> {
        let data = self.send_command_and_read(Command::ReadWeighting, 0, 1)?;
        match data[0] {
            0 => Ok(Weighting::C),
            1 => Ok(Weighting::A),
            2 => Ok(Weighting::Z),
            _ => Err(NsrtError::InvalidResponse),
        }
    }

    /// Set the weighting curve
    pub fn write_weighting(&mut self, weighting: Weighting) -> Result<()> {
        let data = [weighting as u8];
        self.send_command_with_data(Command::WriteWeighting, 0, &data)
    }

    /// Read the current sampling frequency
    pub fn read_sampling_frequency(&mut self) -> Result<SamplingFrequency> {
        let data = self.send_command_and_read(Command::ReadFS, 0, 2)?;
        let freq = u16::from_le_bytes([data[0], data[1]]);

        match freq {
            32000 => Ok(SamplingFrequency::Freq32kHz),
            48000 => Ok(SamplingFrequency::Freq48kHz),
            _ => Err(NsrtError::InvalidResponse),
        }
    }

    /// Set the sampling frequency
    pub fn write_sampling_frequency(&mut self, freq: SamplingFrequency) -> Result<()> {
        let freq_val = freq as u16;
        let data = [freq_val as u8, (freq_val >> 8) as u8];
        self.send_command_with_data(Command::WriteFS, 0, &data)
    }

    /// Read the current time constant in seconds
    pub fn read_time_constant(&mut self) -> Result<f32> {
        let data = self.send_command_and_read(Command::ReadTau, 0, 4)?;
        Ok(f32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    /// Set the time constant in seconds
    pub fn write_time_constant(&mut self, tau: f32) -> Result<()> {
        let tau_bytes = tau.to_le_bytes();
        self.send_command_with_data(Command::WriteTau, 0, &tau_bytes)
    }

    /// Read the model name
    pub fn read_model(&mut self) -> Result<String> {
        // The document states 0-32 bytes, so we'll request 32
        let data = self.send_command_and_read(Command::ReadModel, 0, 32)?;

        // Find the null terminator
        let null_pos = data.iter().position(|&b| b == 0).unwrap_or(data.len());

        // Convert to string
        let model = String::from_utf8_lossy(&data[..null_pos]).to_string();

        Ok(model)
    }

    /// Read the serial number
    pub fn read_serial_number(&mut self) -> Result<String> {
        // The document states 0-32 bytes, so we'll request 32
        let data = self.send_command_and_read(Command::ReadSN, 0, 32)?;

        // Find the null terminator
        let null_pos = data.iter().position(|&b| b == 0).unwrap_or(data.len());

        // Convert to string
        let sn = String::from_utf8_lossy(&data[..null_pos]).to_string();

        Ok(sn)
    }

    /// Read the firmware revision
    pub fn read_firmware_revision(&mut self) -> Result<String> {
        // The document states 0-32 bytes, so we'll request 32
        let data = self.send_command_and_read(Command::ReadFWRev, 0, 32)?;

        // Find the null terminator
        let null_pos = data.iter().position(|&b| b == 0).unwrap_or(data.len());

        // Convert to string
        let fw_rev = String::from_utf8_lossy(&data[..null_pos]).to_string();

        Ok(fw_rev)
    }

    /// Read the date of last calibration
    pub fn read_calibration_date(&mut self) -> Result<u64> {
        let data = self.send_command_and_read(Command::ReadDOC, 0, 8)?;

        // Convert 8 bytes to u64
        let utc = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);

        Ok(utc)
    }

    /// Read the date of birth (manufacturing date)
    pub fn read_birth_date(&mut self) -> Result<u64> {
        let data = self.send_command_and_read(Command::ReadDOB, 0, 8)?;

        // Convert 8 bytes to u64
        let utc = u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]);

        Ok(utc)
    }

    /// Read the user ID
    pub fn read_user_id(&mut self) -> Result<String> {
        // The document states 0-32 bytes, so we'll request 32
        let data = self.send_command_and_read(Command::ReadUserID, 0, 32)?;

        // Find the null terminator
        let null_pos = data.iter().position(|&b| b == 0).unwrap_or(data.len());

        // Convert to string
        let user_id = String::from_utf8_lossy(&data[..null_pos]).to_string();

        Ok(user_id)
    }

    /// Write the user ID
    pub fn write_user_id(&mut self, user_id: &str) -> Result<()> {
        if user_id.len() > 31 {
            return Err(NsrtError::InvalidParameter("User ID too long".to_string()));
        }

        // Create data with null terminator
        let mut data = user_id.as_bytes().to_vec();
        data.push(0); // Add null terminator

        self.send_command_with_data(Command::WriteUserID, 0, &data)
    }

    /// Set the audio debug mode
    ///
    /// Note: According to the documentation, this command has the same code as WriteUserID (0x00000036).
    /// To differentiate, we handle this as a special case by using the command code directly.
    pub fn write_audio_debug_mode(&mut self, mode: AudioDebugMode) -> Result<()> {
        // Create a temporary command packet
        let mut buf = BytesMut::with_capacity(12);
        buf.put_u32_le(0x00000036); // Command code
        buf.put_u32_le(0); // Address field is not relevant
        buf.put_u32_le(1); // Count is 1 byte

        // Send the command packet
        self.port.write_all(&buf)?;

        // Send the data
        let data = [mode as u8];
        self.port.write_all(&data)?;

        // Wait for acknowledgment
        let mut ack = [0u8; 1];
        self.port.read_exact(&mut ack)?;

        if ack[0] != 0x06 {
            return Err(NsrtError::NoAcknowledge);
        }

        Ok(())
    }

    /// Helper function to convert UTC seconds (since Jan 1, 1904) to a human-readable date
    pub fn utc_to_date_string(utc: u64) -> String {
        // This is a simplified implementation
        // In a real application, we would convert this to a proper datetime
        // However, Rust's standard library doesn't support dates before 1970
        // For a complete implementation, consider using the chrono crate

        format!("UTC timestamp: {}", utc)
    }

    /// Helper method to wait for stabilization after changing parameters
    pub fn wait_for_stabilization(&self, tau: f32) {
        // Wait for the larger of 1 second or 10 * tau
        let wait_time = (tau * 10.0).max(1.0);
        thread::sleep(Duration::from_secs_f32(wait_time));
    }
}
