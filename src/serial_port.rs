use core::slice;
use usb_device::class_prelude::*;
use usb_device::Result;
use generic_array::ArrayLength;
use generic_array::typenum;
use crate::cdc_acm::*;
use crate::buffer::Buffer;

#[derive(Eq, PartialEq)]
enum WriteState {
    /// Not currently writing anything.
    Idle = 0,

    /// Writing a short packet.
    WriteShort = 1,

    /// Writing a full packet that needs to be followed by a short packet.
    WriteFull = 2,
}

/// USB serial port (CDC-ACM) class with built-in buffering.
pub struct SerialPort<'a, B, NRBUF=typenum::U128, NWBUF=typenum::U128>
where
    B: UsbBus,
    NRBUF: ArrayLength<u8>,
    NWBUF: ArrayLength<u8>,
{
    inner: CdcAcmClass<'a, B>,
    read_buf: Buffer<NRBUF>,
    write_buf: Buffer<NWBUF>,
    write_state: WriteState,
}

impl<B, NRBUF, NWBUF> SerialPort<'_, B, NRBUF, NWBUF>
where
    B: UsbBus,
    NRBUF: ArrayLength<u8>,
    NWBUF: ArrayLength<u8>,
{
    /// Creates a new USB serial port with the provided UsbBus.
    pub fn new(alloc: &UsbBusAllocator<B>) -> SerialPort<'_, B, NRBUF, NWBUF> {
        SerialPort {
            inner: CdcAcmClass::new(alloc, 64),
            write_buf: Buffer::new(),
            read_buf: Buffer::new(),
            write_state: WriteState::Idle,
        }
    }

    /// Gets the current line coding.
    pub fn line_coding(&self) -> &LineCoding { self.inner.line_coding() }

    /// Gets the DTR (data terminal ready) state
    pub fn dtr(&self) -> bool { self.inner.dtr() }

    /// Gets the RTS (ready to send) state
    pub fn rts(&self) -> bool { self.inner.rts() }

    /// Writes bytes from `data` into the port and returns the number of bytes written.
    pub fn write(&mut self, data: &[u8]) -> Result<usize> {
        if self.write_buf.available_write() == 0 {
            // Buffer is full, try to flush

            match self.flush() {
                Ok(_) | Err(UsbError::WouldBlock) => { },
                Err(err) => { return Err(err); },
            };

            if self.write_buf.available_write() == 0 {
                // Still full, can't write anything.
                return Ok(0);
            }
        }

        Ok(self.write_buf.write(data))
    }

    /// Reads bytes from the port and returns the number of bytes read into `data`.
    pub fn read(&mut self, data: &mut [u8]) -> Result<usize> {
        let buf = &mut self.read_buf;
        let inner = &mut self.inner;

        // Try to read a packet from the endpoint and write it into the buffer if it fits. Propagate
        // errors except `WouldBlock`.

        buf.write_all(inner.max_packet_size() as usize, |buf_data| {
            match inner.read_packet(buf_data) {
                Ok(c) => Ok(c),
                Err(UsbError::WouldBlock) => Ok(0),
                Err(err) => Err(err),
            }
        })?;

        if buf.available_read() == 0 {
            // No data available for reading.
            return Ok(0);
        }

        let r = buf.read(data.len(), |buf_data| {
            &data[..buf_data.len()].copy_from_slice(buf_data);

            Ok(buf_data.len())
        });

        r
    }

    /// Sends as much as possible of the current write buffer. Returns `Ok` if the write buffer has
    /// been completely transferred to and acknowledged by the host, `Err(WouldBlock)` if there is
    /// still unacknowledged data, and other errors if there's an error sending data to the host.
    pub fn flush(&mut self) -> Result<()> {
        let buf = &mut self.write_buf;

        if buf.available_read() > 0 {
            let inner = &mut self.inner;
            let write_state = &mut self.write_state;

            buf.read(inner.max_packet_size() as usize, |buf_data| {
                match inner.write_packet(buf_data) {
                    Ok(_) => {
                        *write_state = if buf_data.len() == inner.max_packet_size() as usize {
                            WriteState::WriteFull
                        } else {
                            WriteState::WriteShort
                        };

                        Ok(())
                    },
                    Err(UsbError::WouldBlock) => Ok(()),
                    Err(err) => Err(err),
                }
            })?;
        }

        if self.write_state == WriteState::Idle {
            Ok(())
        } else {
            Err(UsbError::WouldBlock)
        }
    }
}

impl<B, NRBUF, NWBUF> UsbClass<B> for SerialPort<'_, B, NRBUF, NWBUF>
where
    B: UsbBus,
    NRBUF: ArrayLength<u8>,
    NWBUF: ArrayLength<u8>,
{
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        self.inner.get_configuration_descriptors(writer)
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.read_buf.clear();
        self.write_buf.clear();
        self.write_state = WriteState::Idle;
    }

    fn poll(&mut self) {
        self.flush().ok();
    }

    fn endpoint_in_complete(&mut self, addr: EndpointAddress) {
        if addr == self.inner.write_ep.address() {
            match self.write_state {
                WriteState::WriteFull => {
                    self.write_state = WriteState::WriteShort;
                    self.inner.write_packet(&[]).ok();
                },
                WriteState::WriteShort => {
                    self.write_state = WriteState::Idle;
                },
                WriteState::Idle => { },
            }
        }
    }

    fn control_in(&mut self, xfer: ControlIn<B>) { self.inner.control_in(xfer); }

    fn control_out(&mut self, xfer: ControlOut<B>) { self.inner.control_out(xfer); }
}

impl<B, NRBUF, NWBUF> embedded_hal::serial::Write<u8> for SerialPort<'_, B, NRBUF, NWBUF>
where
    B: UsbBus,
    NRBUF: ArrayLength<u8>,
    NWBUF: ArrayLength<u8>,
{
    type Error = UsbError;

    fn write(&mut self, word: u8) -> nb::Result<(), Self::Error> {
        match <SerialPort<'_, B, NRBUF, NWBUF>>::write(self, slice::from_ref(&word)) {
            Ok(0) | Err(UsbError::WouldBlock) => Err(nb::Error::WouldBlock),
            Ok(_) => Ok(()),
            Err(err) => Err(nb::Error::Other(err)),
        }
    }

    fn flush(&mut self) -> nb::Result<(), Self::Error> {
        match <SerialPort<'_, B, NRBUF, NWBUF>>::flush(self) {
            Err(UsbError::WouldBlock) => Err(nb::Error::WouldBlock),
            Ok(_) => Ok(()),
            Err(err) => Err(nb::Error::Other(err)),
        }
    }
}

impl<B, NRBUF, NWBUF> embedded_hal::serial::Read<u8> for SerialPort<'_, B, NRBUF, NWBUF>
where
    B: UsbBus,
    NRBUF: ArrayLength<u8>,
    NWBUF: ArrayLength<u8>,
{
    type Error = UsbError;

    fn read(&mut self) -> nb::Result<u8, Self::Error> {
        let mut buf: u8 = 0;

        match <SerialPort<'_, B, NRBUF, NWBUF>>::read(self, slice::from_mut(&mut buf)) {
            Ok(0) | Err(UsbError::WouldBlock) => Err(nb::Error::WouldBlock),
            Ok(_) => Ok(buf),
            Err(err) => Err(nb::Error::Other(err)),
        }
    }
}
