use crate::buffer::{Buffer, DefaultBufferStore};
use crate::cdc_acm::*;
use core::borrow::BorrowMut;
use core::slice;
use core::task::Waker;
use usb_device::class_prelude::*;
use usb_device::descriptor::lang_id::LangID;
use usb_device::Result;

/// USB (CDC-ACM) serial port with built-in buffering to implement stream-like behavior.
///
/// The RS and WS type arguments specify the storage for the read/write buffers, respectively. By
/// default an internal 128 byte buffer is used for both directions.
pub struct SerialPort<'a, B, RS = DefaultBufferStore, WS = DefaultBufferStore>
where
    B: UsbBus,
    RS: BorrowMut<[u8]>,
    WS: BorrowMut<[u8]>,
{
    inner: CdcAcmClass<'a, B>,
    pub(crate) read_buf: Buffer<RS>,
    pub(crate) write_buf: Buffer<WS>,
    write_state: WriteState,

    pub(crate) read_waker: Option<Waker>,
    pub(crate) write_waker: Option<Waker>,
}

/// If this many full size packets have been sent in a row, a short packet will be sent so that the
/// host sees the data in a timely manner.
const SHORT_PACKET_INTERVAL: usize = 10;

/// Keeps track of the type of the last written packet.
enum WriteState {
    /// No packets in-flight
    Idle,

    /// Short packet currently in-flight
    Short,

    /// Full packet current in-flight. A full packet must be followed by a short packet for the host
    /// OS to see the transaction. The data is the number of subsequent full packets sent so far. A
    /// short packet is forced every SHORT_PACKET_INTERVAL packets so that the OS sees data in a
    /// timely manner.
    Full(usize),
}

impl<'a, B> SerialPort<'a, B>
where
    B: UsbBus,
{
    /// Creates a new USB serial port with the provided UsbBus and 128 byte read/write buffers.
    pub fn new<'alloc: 'a>(
        alloc: &'alloc UsbBusAllocator<B>,
    ) -> SerialPort<'a, B, DefaultBufferStore, DefaultBufferStore> {
        Self::new_with_interface_names(alloc, None, None)
    }
    /// Same as SerialPort::new, but allows specifying the names of the interfaces
    pub fn new_with_interface_names<'alloc: 'a>(
        alloc: &'alloc UsbBusAllocator<B>,
        comm_if_name: Option<&'static str>,
        data_if_name: Option<&'static str>,
    ) -> SerialPort<'a, B, DefaultBufferStore, DefaultBufferStore> {
        SerialPort::new_with_store_and_interface_names(
            alloc,
            DefaultBufferStore::default(),
            DefaultBufferStore::default(),
            comm_if_name,
            data_if_name,
        )
    }
}

impl<'a, B, RS, WS> SerialPort<'a, B, RS, WS>
where
    B: UsbBus,
    RS: BorrowMut<[u8]>,
    WS: BorrowMut<[u8]>,
{
    /// Creates a new USB serial port with the provided UsbBus and buffer backing stores.
    pub fn new_with_store<'alloc: 'a>(
        alloc: &'alloc UsbBusAllocator<B>,
        read_store: RS,
        write_store: WS,
    ) -> SerialPort<'a, B, RS, WS> {
        Self::new_with_store_and_interface_names(alloc, read_store, write_store, None, None)
    }

    /// Creates a new USB serial port with the provided UsbBus and buffer backing stores.
    pub fn new_with_store_and_interface_names<'alloc: 'a>(
        alloc: &'alloc UsbBusAllocator<B>,
        read_store: RS,
        write_store: WS,
        comm_if_name: Option<&'static str>,
        data_if_name: Option<&'static str>,
    ) -> SerialPort<'a, B, RS, WS> {
        SerialPort {
            inner: CdcAcmClass::new_with_interface_names(alloc, 64, comm_if_name, data_if_name),
            read_buf: Buffer::new(read_store),
            write_buf: Buffer::new(write_store),
            write_state: WriteState::Idle,
            read_waker: None,
            write_waker: None,
        }
    }

    /// Gets the current line coding.
    pub fn line_coding(&self) -> &LineCoding {
        self.inner.line_coding()
    }

    /// Gets the DTR (data terminal ready) state
    pub fn dtr(&self) -> bool {
        self.inner.dtr()
    }

    /// Gets the RTS (request to send) state
    pub fn rts(&self) -> bool {
        self.inner.rts()
    }

    /// Writes bytes from `data` into the port and returns the number of bytes written.
    ///
    /// # Errors
    ///
    /// * [`WouldBlock`](usb_device::UsbError::WouldBlock) - No bytes could be written because the
    ///   buffers are full.
    ///
    /// Other errors from `usb-device` may also be propagated.
    pub fn write(&mut self, data: &[u8]) -> Result<usize> {
        let count = self.write_buf.write(data);

        match self.flush() {
            Ok(_) | Err(UsbError::WouldBlock) => {}
            Err(err) => {
                return Err(err);
            }
        };

        if count == 0 {
            Err(UsbError::WouldBlock)
        } else {
            Ok(count)
        }
    }

    /// Poll the endpoint and try to put them into the serial buffer.
    pub(crate) fn poll(&mut self) -> Result<()> {
        let Self {
            inner, read_buf, ..
        } = self;

        read_buf.write_all(inner.max_packet_size() as usize, |buf_data| {
            match inner.read_packet(buf_data) {
                Ok(c) => Ok(c),
                Err(UsbError::WouldBlock) => Ok(0),
                Err(err) => Err(err),
            }
        })?;
        if let Some(read_waker) = self.read_waker.take() {
            read_waker.wake();
        }

        Ok(())
    }

    /// Reads bytes from the port into `data` and returns the number of bytes read.
    ///
    /// # Errors
    ///
    /// * [`WouldBlock`](usb_device::UsbError::WouldBlock) - No bytes available for reading.
    ///
    /// Other errors from `usb-device` may also be propagated.
    pub fn read(&mut self, data: &mut [u8]) -> Result<usize> {
        // Try to read a packet from the endpoint and write it into the buffer if it fits. Propagate
        // errors except `WouldBlock`.
        self.poll()?;

        if self.read_buf.available_read() == 0 {
            // No data available for reading.
            return Err(UsbError::WouldBlock);
        }

        self.read_buf.read(data.len(), |buf_data| {
            data[..buf_data.len()].copy_from_slice(buf_data);

            Ok(buf_data.len())
        })
    }

    /// Sends as much as possible of the current write buffer. Returns `Ok` if all data that has
    /// been written has been completely written to hardware buffers `Err(WouldBlock)` if there is
    /// still data remaining, and other errors if there's an error sending data to the host. Note
    /// that even if this method returns `Ok`, data may still be in hardware buffers on either side.
    pub fn flush(&mut self) -> Result<()> {
        let buf = &mut self.write_buf;
        let inner = &mut self.inner;
        let write_state = &mut self.write_state;

        let full_count = match *write_state {
            WriteState::Full(c) => c,
            _ => 0,
        };

        if buf.available_read() > 0 {
            // There's data in the write_buf, so try to write that first.

            let max_write_size = if full_count >= SHORT_PACKET_INTERVAL {
                inner.max_packet_size() - 1
            } else {
                inner.max_packet_size()
            } as usize;

            buf.read(max_write_size, |buf_data| {
                // This may return WouldBlock which will be propagated.
                inner.write_packet(buf_data)?;

                *write_state = if buf_data.len() == inner.max_packet_size() as usize {
                    WriteState::Full(full_count + 1)
                } else {
                    WriteState::Short
                };

                Ok(buf_data.len())
            })?;

            Err(UsbError::WouldBlock)
        } else if full_count != 0 {
            // Write a ZLP to complete the transaction if there's nothing else to write and the last
            // packet was a full one. This may return WouldBlock which will be propagated.
            inner.write_packet(&[])?;

            *write_state = WriteState::Short;

            Err(UsbError::WouldBlock)
        } else {
            // No data left in writer_buf.

            *write_state = WriteState::Idle;

            Ok(())
        }
    }
}

impl<B, RS, WS> UsbClass<B> for SerialPort<'_, B, RS, WS>
where
    B: UsbBus,
    RS: BorrowMut<[u8]>,
    WS: BorrowMut<[u8]>,
{
    fn get_configuration_descriptors(&self, writer: &mut DescriptorWriter) -> Result<()> {
        self.inner.get_configuration_descriptors(writer)
    }

    fn get_string(&self, index: StringIndex, lang_id: LangID) -> Option<&str> {
        self.inner.get_string(index, lang_id)
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.read_buf.clear();
        self.write_buf.clear();
        self.write_state = WriteState::Idle;
    }

    fn endpoint_in_complete(&mut self, addr: EndpointAddress) {
        if addr == self.inner.write_ep().address() {
            self.flush().ok();
            if let Some(write_waker) = self.write_waker.take() {
                write_waker.wake();
            }
        }
    }

    fn control_in(&mut self, xfer: ControlIn<B>) {
        self.inner.control_in(xfer);
    }

    fn control_out(&mut self, xfer: ControlOut<B>) {
        self.inner.control_out(xfer);
    }
}

impl<B, RS, WS> embedded_hal::serial::Write<u8> for SerialPort<'_, B, RS, WS>
where
    B: UsbBus,
    RS: BorrowMut<[u8]>,
    WS: BorrowMut<[u8]>,
{
    type Error = UsbError;

    fn write(&mut self, word: u8) -> nb::Result<(), Self::Error> {
        match <SerialPort<'_, B, RS, WS>>::write(self, slice::from_ref(&word)) {
            Ok(0) | Err(UsbError::WouldBlock) => Err(nb::Error::WouldBlock),
            Ok(_) => Ok(()),
            Err(err) => Err(nb::Error::Other(err)),
        }
    }

    fn flush(&mut self) -> nb::Result<(), Self::Error> {
        match <SerialPort<'_, B, RS, WS>>::flush(self) {
            Err(UsbError::WouldBlock) => Err(nb::Error::WouldBlock),
            Ok(_) => Ok(()),
            Err(err) => Err(nb::Error::Other(err)),
        }
    }
}

impl<B, RS, WS> embedded_hal::serial::Read<u8> for SerialPort<'_, B, RS, WS>
where
    B: UsbBus,
    RS: BorrowMut<[u8]>,
    WS: BorrowMut<[u8]>,
{
    type Error = UsbError;

    fn read(&mut self) -> nb::Result<u8, Self::Error> {
        let mut buf: u8 = 0;

        match <SerialPort<'_, B, RS, WS>>::read(self, slice::from_mut(&mut buf)) {
            Ok(0) | Err(UsbError::WouldBlock) => Err(nb::Error::WouldBlock),
            Ok(_) => Ok(buf),
            Err(err) => Err(nb::Error::Other(err)),
        }
    }
}
