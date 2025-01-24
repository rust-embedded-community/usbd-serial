use super::SerialPort;
use core::{
    borrow::BorrowMut,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
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

impl<Bus: UsbBus, RS: BorrowMut<[u8]>, WS: BorrowMut<[u8]>> embedded_io::ErrorType
    for SerialPort<'_, Bus, RS, WS>
{
    type Error = Error;
}

impl<Bus: UsbBus, RS: BorrowMut<[u8]>, WS: BorrowMut<[u8]>> embedded_io::Read
    for SerialPort<'_, Bus, RS, WS>
{
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

impl<Bus: UsbBus, RS: BorrowMut<[u8]>, WS: BorrowMut<[u8]>> embedded_io::ReadReady
    for SerialPort<'_, Bus, RS, WS>
{
    fn read_ready(&mut self) -> Result<bool, Self::Error> {
        self.poll()?;
        Ok(self.read_buf.available_read() != 0)
    }
}

impl<Bus: UsbBus, RS: BorrowMut<[u8]>, WS: BorrowMut<[u8]>> embedded_io::Write
    for SerialPort<'_, Bus, RS, WS>
{
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        if buf.is_empty() {
            return Ok(0);
        }

        loop {
            match self.write(buf) {
                // We are required by `embedded-io` to continue writing until at least one byte is
                // written.
                Ok(0) => {}
                Err(usb_device::UsbError::WouldBlock) => {}
                other => return Ok(other?),
            }
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.flush().map_err(From::from)
    }
}

impl<Bus: UsbBus, RS: BorrowMut<[u8]>, WS: BorrowMut<[u8]>> embedded_io::WriteReady
    for SerialPort<'_, Bus, RS, WS>
{
    fn write_ready(&mut self) -> Result<bool, Self::Error> {
        Ok(self.write_buf.available_write() != 0)
    }
}

impl<B, RS, WS> embedded_io_async::Write for SerialPort<'_, B, RS, WS>
where
    B: UsbBus,
    RS: BorrowMut<[u8]>,
    WS: BorrowMut<[u8]>,
{
    async fn write(&mut self, buffer: &[u8]) -> core::result::Result<usize, Self::Error> {
        if buffer.is_empty() {
            return Ok(0);
        }
        AsyncWrite {
            serial_port: self,
            buffer,
        }
        .await
    }

    // async fn flush(&mut self) -> core::result::Result<(), Self::Error> {
    //     todo!()
    // }
}
struct AsyncWrite<'a, 'b, 'c, B, RS, WS>
where
    B: UsbBus,
    RS: BorrowMut<[u8]>,
    WS: BorrowMut<[u8]>,
{
    serial_port: &'a mut SerialPort<'b, B, RS, WS>,
    buffer: &'c [u8],
}

impl<'a, 'b, 'c, B, RS, WS> Future for AsyncWrite<'a, 'b, 'c, B, RS, WS>
where
    B: UsbBus,
    RS: BorrowMut<[u8]>,
    WS: BorrowMut<[u8]>,
{
    type Output = Result<usize, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let s = self.get_mut();
        match s.serial_port.write(&s.buffer) {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(usb_device::UsbError::WouldBlock) => {
                // No need to worry about overriding.
                // The ownership is borrowed though the mutable reference,
                // so it's impossable to run twice at the same time.
                s.serial_port.write_waker = Some(cx.waker().clone());
                Poll::Pending
            }
            Err(err) => Poll::Ready(Err(Error(err))),
        }
    }
}

impl<B, RS, WS> embedded_io_async::Read for SerialPort<'_, B, RS, WS>
where
    B: UsbBus,
    RS: BorrowMut<[u8]>,
    WS: BorrowMut<[u8]>,
{
    async fn read(&mut self, buffer: &mut [u8]) -> Result<usize, Self::Error> {
        AsyncRead {
            serial_port: self,
            buffer,
        }
        .await
    }
}

struct AsyncRead<'a, 'b, 'c, B, RS, WS>
where
    B: UsbBus,
    RS: BorrowMut<[u8]>,
    WS: BorrowMut<[u8]>,
{
    serial_port: &'a mut SerialPort<'b, B, RS, WS>,
    buffer: &'c mut [u8],
}

impl<'a, 'b, 'c, B, RS, WS> Future for AsyncRead<'a, 'b, 'c, B, RS, WS>
where
    B: UsbBus,
    RS: BorrowMut<[u8]>,
    WS: BorrowMut<[u8]>,
{
    type Output = Result<usize, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let s = self.get_mut();
        match s.serial_port.read(&mut s.buffer) {
            Ok(n) => Poll::Ready(Ok(n)),
            Err(usb_device::UsbError::WouldBlock) => {
                if s.buffer.len() == 0 {
                    Poll::Ready(Ok(0))
                } else {
                    // No need to worry about overriding.
                    // The ownership is borrowed though the mutable reference,
                    // so it's impossable to run twice at the same time.
                    s.serial_port.read_waker = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
            Err(err) => Poll::Ready(Err(Error(err))),
        }
    }
}
