use serialport::SerialPort;
use std::{
    ffi::CStr,
    io::{Read, Write},
    thread,
    time::Duration,
};
use thiserror::Error;

const VID: u16 = 2649;
const PID: u16 = 323;

/// Error type for the `NSRT_mk4` driver
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

    #[error("FromBytesUntilNul error: {0}")]
    FromBytesUntilNulError(#[from] std::ffi::FromBytesUntilNulError),

    #[error("Utf8 error: {0}")]
    Utf8Error(#[from] std::str::Utf8Error),
}

/// Result type for the `NSRT_mk4` driver
pub type Result<T> = std::result::Result<T, NsrtError>;

/// Weighting functions supported by the `NSRT_mk4`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Weighting {
    /// C-weighting (dB-C)
    C = 0,
    /// A-weighting (dB-A)
    A = 1,
    /// Z-weighting (dB-Z) - flat frequency response
    Z = 2,
}

/// Sampling frequencies supported by the `NSRT_mk4`
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplingFrequency {
    /// 32 kHz
    Freq32kHz = 32000,
    /// 48 kHz
    Freq48kHz = 48000,
}

/// Command codes for the `NSRT_mk4` device
#[derive(Debug, Clone, Copy)]
#[repr(u32)]
enum Command {
    ReadLevel = 0x8000_0010,
    ReadLEQ = 0x8000_0011,
    ReadTemperature = 0x8000_0012,
    ReadWeighting = 0x8000_0020,
    ReadFS = 0x8000_0021,
    ReadTau = 0x8000_0022,
    ReadModel = 0x8000_0031,
    ReadSN = 0x8000_0032,
    ReadFWRev = 0x8000_0033,
    ReadDOC = 0x8000_0034,
    ReadDOB = 0x8000_0035,
    ReadUserID = 0x8000_0036,
    WriteWeighting = 0x0000_0020,
    WriteFS = 0x0000_0021,
    WriteTau = 0x0000_0022,
    WriteUserID = 0x0000_0036,
}

/// Command packet structure
#[derive(Debug)]
struct CommandPacket {
    command: u32,
    address: u32,
    count: u32,
}

impl CommandPacket {
    fn serialize(&self) -> Vec<u8> {
        let mut result = Vec::with_capacity(12);
        result.extend_from_slice(&self.command.to_le_bytes());
        result.extend_from_slice(&self.address.to_le_bytes());
        result.extend_from_slice(&self.count.to_le_bytes());
        result
    }
}

/// The main driver for the `NSRT_mk4` device
pub struct NSRT {
    port: Box<dyn SerialPort>,
}

impl NSRT {
    /// Apply stabilization wait after configuration
    ///
    /// Call this after performing multiple chained configuration methods
    /// to apply a single stabilization wait.
    #[must_use = "This method returns the updated NSRT instance which should be used for further operations"]
    pub fn apply(mut self) -> Result<Self> {
        let tau = self.read_time_constant()?;
        Self::wait_for_stabilization(tau);
        Ok(self)
    }

    /// Open the `NSRT_mk4` device
    ///
    /// This method automatically finds and opens the first `NSRT_mk4` device
    /// connected to the system using the Convergence Instruments VID/PID.
    pub fn open() -> Result<Self> {
        let ports = serialport::available_ports()?;

        for port_info in ports {
            if let serialport::SerialPortType::UsbPort(usb_info) = &port_info.port_type
                && usb_info.vid == VID
                && usb_info.pid == PID
            {
                let port = serialport::new(&port_info.port_name, 9600)
                    .timeout(Duration::from_millis(1000))
                    .open()?;

                return Ok(Self { port });
            }
        }

        Err(NsrtError::NoDevice)
    }

    /// Send a command to the device
    fn send_command(&mut self, cmd: Command, address: u32, count: u32) -> Result<()> {
        let packet = CommandPacket {
            command: cmd as u32,
            address,
            count,
        };

        let bytes = packet.serialize();
        self.port.write_all(&bytes)?;

        Ok(())
    }

    /// Send a command with data to the device
    fn send_command_with_data(&mut self, cmd: Command, address: u32, data: &[u8]) -> Result<()> {
        self.send_command(
            cmd,
            address,
            u32::try_from(data.len()).map_err(|_| {
                NsrtError::InvalidParameter("Data too large for command".to_string())
            })?,
        )?;

        self.port.write_all(data)?;

        let mut ack = [0u8; 1];
        self.port.read_exact(&mut ack)?;

        if ack[0] != 0x06 {
            return Err(NsrtError::NoAcknowledge);
        }

        Ok(())
    }

    /// Send a command and read response data
    fn send_command_and_read(&mut self, cmd: Command, address: u32, count: u32) -> Result<Vec<u8>> {
        self.send_command(cmd, address, count)?;

        let mut response = vec![0u8; count as usize];
        self.port.read_exact(&mut response)?;

        Ok(response)
    }

    /// Read the current sound level in dB
    pub fn read_level(&mut self) -> Result<f32> {
        let data = self.send_command_and_read(Command::ReadLevel, 0, 4)?;
        if data.len() < 4 {
            return Err(NsrtError::InvalidResponse);
        }
        Ok(f32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    /// Read the current LEQ (Equivalent Continuous Sound Level) in dB
    /// and restart integration for the next LEQ measurement
    pub fn read_leq(&mut self) -> Result<f32> {
        let data = self.send_command_and_read(Command::ReadLEQ, 0, 4)?;
        if data.len() < 4 {
            return Err(NsrtError::InvalidResponse);
        }
        Ok(f32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    /// Read the current temperature in degrees Celsius
    pub fn read_temperature(&mut self) -> Result<f32> {
        let data = self.send_command_and_read(Command::ReadTemperature, 0, 4)?;
        if data.len() < 4 {
            return Err(NsrtError::InvalidResponse);
        }
        Ok(f32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    /// Read the current weighting curve
    pub fn read_weighting(&mut self) -> Result<Weighting> {
        let data = self.send_command_and_read(Command::ReadWeighting, 0, 1)?;
        if data.is_empty() {
            return Err(NsrtError::InvalidResponse);
        }
        match data.first() {
            Some(0) => Ok(Weighting::C),
            Some(1) => Ok(Weighting::A),
            Some(2) => Ok(Weighting::Z),
            _ => Err(NsrtError::InvalidResponse),
        }
    }

    /// Set the weighting curve
    ///
    /// After setting the weighting, this automatically waits for the device to stabilize
    /// unless `skip_wait` is set to true (useful when changing multiple parameters).
    fn write_weighting(&mut self, weighting: Weighting, skip_wait: bool) -> Result<()> {
        let data = [(weighting as u8)];
        self.send_command_with_data(Command::WriteWeighting, 0, &data)?;

        if !skip_wait {
            let tau = self.read_time_constant()?;
            Self::wait_for_stabilization(tau);
        }

        Ok(())
    }

    /// Set the weighting curve using fluent API
    ///
    /// This method can be chained with other setters during initialization.
    #[must_use = "This method returns the updated NSRT instance which should be used for further operations"]
    pub fn weighting(mut self, weighting: Weighting) -> Result<Self> {
        self.write_weighting(weighting, true)?;
        Ok(self)
    }

    /// Read the current sampling frequency
    pub fn read_sampling_frequency(&mut self) -> Result<SamplingFrequency> {
        let data = self.send_command_and_read(Command::ReadFS, 0, 2)?;
        if data.len() < 2 {
            return Err(NsrtError::InvalidResponse);
        }
        let value = u16::from_le_bytes([data[0], data[1]]);
        match value {
            32000 => Ok(SamplingFrequency::Freq32kHz),
            48000 => Ok(SamplingFrequency::Freq48kHz),
            _ => Err(NsrtError::InvalidResponse),
        }
    }

    /// Set the sampling frequency
    ///
    /// After setting the sampling frequency, this automatically waits for the device to stabilize
    /// unless `skip_wait` is set to true (useful when changing multiple parameters).
    fn write_sampling_frequency(&mut self, freq: SamplingFrequency) -> Result<()> {
        let data = (freq as u16).to_le_bytes();
        self.send_command_with_data(Command::WriteFS, 0, &data)?;
        Self::wait_for_stabilization(self.read_time_constant()?);
        Ok(())
    }

    /// Set the sampling frequency using fluent API
    #[must_use = "This method returns the updated NSRT instance which should be used for further operations"]
    pub fn sampling_frequency(mut self, freq: SamplingFrequency) -> Result<Self> {
        self.write_sampling_frequency(freq)?;
        Ok(self)
    }

    /// Read the current time constant in seconds
    pub fn read_time_constant(&mut self) -> Result<f32> {
        let data = self.send_command_and_read(Command::ReadTau, 0, 4)?;
        if data.len() < 4 {
            return Err(NsrtError::InvalidResponse);
        }
        Ok(f32::from_le_bytes([data[0], data[1], data[2], data[3]]))
    }

    /// Set the time constant in seconds
    ///
    /// After setting the time constant, this automatically waits for the device to stabilize
    /// unless `skip_wait` is set to true (useful when changing multiple parameters).
    fn write_time_constant(&mut self, tau: f32, skip_wait: bool) -> Result<()> {
        let data = tau.to_le_bytes();
        self.send_command_with_data(Command::WriteTau, 0, &data)?;

        if !skip_wait {
            Self::wait_for_stabilization(tau);
        }

        Ok(())
    }

    /// Set the time constant using fluent API
    ///
    /// This method can be chained with other setters during initialization.
    #[must_use = "This method returns the updated NSRT instance which should be used for further operations"]
    pub fn time_constant(mut self, tau: f32) -> Result<Self> {
        self.write_time_constant(tau, true)?;
        Ok(self)
    }

    /// Read the model name
    pub fn read_model(&mut self) -> Result<String> {
        let data = self.send_command_and_read(Command::ReadModel, 0, 32)?;
        Ok(CStr::from_bytes_until_nul(&data)?.to_str()?.to_string())
    }

    /// Read the serial number
    pub fn read_serial_number(&mut self) -> Result<String> {
        let data = self.send_command_and_read(Command::ReadSN, 0, 32)?;
        Ok(CStr::from_bytes_until_nul(&data)?.to_str()?.to_string())
    }

    /// Read the firmware revision
    pub fn read_firmware_revision(&mut self) -> Result<String> {
        let data = self.send_command_and_read(Command::ReadFWRev, 0, 32)?;
        Ok(CStr::from_bytes_until_nul(&data)?.to_str()?.to_string())
    }

    /// Read the date of last calibration
    pub fn read_calibration_date(&mut self) -> Result<u64> {
        let data = self.send_command_and_read(Command::ReadDOC, 0, 8)?;
        if data.len() < 8 {
            return Err(NsrtError::InvalidResponse);
        }
        Ok(u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]))
    }

    /// Read the date of birth (manufacturing date)
    pub fn read_birth_date(&mut self) -> Result<u64> {
        let data = self.send_command_and_read(Command::ReadDOB, 0, 8)?;
        if data.len() < 8 {
            return Err(NsrtError::InvalidResponse);
        }
        Ok(u64::from_le_bytes([
            data[0], data[1], data[2], data[3], data[4], data[5], data[6], data[7],
        ]))
    }

    /// Read the user ID
    pub fn read_user_id(&mut self) -> Result<String> {
        let data = self.send_command_and_read(Command::ReadUserID, 0, 32)?;
        Ok(CStr::from_bytes_until_nul(&data)?.to_str()?.to_string())
    }

    /// Write the user ID
    #[allow(dead_code)]
    fn write_user_id(&mut self, user_id: &str) -> Result<()> {
        if user_id.len() > 31 {
            return Err(NsrtError::InvalidParameter("User ID too long".to_string()));
        }

        let mut data = user_id.as_bytes().to_vec();
        data.push(0);

        self.send_command_with_data(Command::WriteUserID, 0, &data)
    }

    /// Helper method to wait for stabilization after changing parameters
    fn wait_for_stabilization(tau: f32) {
        let wait_time = (tau * 10.0).max(1.0);
        thread::sleep(Duration::from_secs_f32(wait_time));
    }
}
