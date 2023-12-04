use super::SerialPort;
use usb_device::bus::UsbBus;

#[derive(Debug)]
pub struct Error(usb_device::UsbError);

impl From<usb_device::UsbError> for Error {
    fn from(e: usb_device::UsbError) -> Self {
        Self(e)
    }
}

impl embedded_io::Error for Error {
    fn kind(&self) -> embedded_io::ErrorKind {
        match self.0 {
            usb_device::UsbError::Unsupported => embedded_io::ErrorKind::Unsupported,
            usb_device::UsbError::BufferOverflow
            | usb_device::UsbError::EndpointOverflow
            | usb_device::UsbError::EndpointMemoryOverflow => embedded_io::ErrorKind::OutOfMemory,
            _ => embedded_io::ErrorKind::Other,
        }
    }
}

impl<Bus: UsbBus> embedded_io::ErrorType for SerialPort<'_, Bus> {
    type Error = Error;
}

impl<Bus: UsbBus> embedded_io::Read for SerialPort<'_, Bus> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        loop {
            match self.read(buf).map_err(From::from) {
                // We are required by `embedded-io` to continue reading until at least one byte is
                // read.
                Ok(0) => {}
                Err(usb_device::UsbError::WouldBlock) => {}
                other => return Ok(other?),
            }
        }
    }
}

impl<Bus: UsbBus> embedded_io::ReadReady for SerialPort<'_, Bus> {
    fn read_ready(&mut self) -> Result<bool, Self::Error> {
        self.poll()?;
        Ok(self.read_buf.available_read() != 0)
    }
}

impl<Bus: UsbBus> embedded_io::Write for SerialPort<'_, Bus> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() {
            return Ok(0);
        }

        loop {
            match self.write(buf) {
                // We are required by `embedded-io` to continue writing until at least one byte is
                // written.
                Ok(0) => {
                    log::info!("write::0");
                }
                Err(usb_device::UsbError::WouldBlock) => {
                    log::info!("write::WouldBlock");
                }
                Ok(n) => {
                    log::info!("write::{n}");
                    return Ok(n)
                }
                other => {
                    log::info!("write::other");
                    return Ok(other?)
                }
            }
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.flush().map_err(From::from)
    }
}

impl<Bus: UsbBus> embedded_io::WriteReady for SerialPort<'_, Bus> {
    fn write_ready(&mut self) -> Result<bool, Self::Error> {
        log::info!("WriteAVail: {}", self.write_buf.available_write());
        Ok(self.write_buf.available_write() != 0)
    }
}
