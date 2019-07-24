//! CDC-ACM USB serial port implementation for [usb-device](https://crates.io/crates/usb-device).
//!
//! CDC-ACM is a USB class that's supported out of the box by most operating systems and used for
//! implementing modems and generic serial ports. The [`SerialPort`](crate::SerialPort) class
//! implements a stream-like buffered serial port that can be used similarly to a normal UART.
//!
//! The crate also contains [`CdcAcmClass`](CdcAcmClass) which is a lower-level implementation that
//! has less overhead, but requires more care to use correctly.
//!
//! Example
//! =======
//!
//! A full example requires the use of a hardware-driver, but the hardware independent part is as
//! follows:
//!
//! ```
//! let mut serial = SerialPort::new(&usb_bus);
//!
//! let mut usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x16c0, 0x27dd))
//!     .product("Serial port")
//!     .device_class(USB_CLASS_CDC)
//!     .build();
//!
//! loop {
//!     if !usb_dev.poll(&mut [&mut serial]) {
//!         continue;
//!     }
//!
//!     let mut buf = [0u8; 64];
//!
//!     match serial.read(&mut buf[..]) {
//!         Ok(count) => {
//!             if count > 0 {
//!                 // Received data is in &buf[..count]
//!             }
//!         },
//!         Err(err) => // An error occurred
//!     };
//!
//!     match serial.write(&[0x3a, 0x29]) {
//!         Ok(count) => {
//!             // count bytes were written (may be 0)
//!         },
//!         //Err(UsbError::WouldBlock) => { },
//!         Err(err) => // An error occurred
//!     };
//! }
//! ```

#![no_std]

mod buffer;
mod cdc_acm;
mod serial_port;

pub use usb_device::{Result, UsbError};
pub use crate::cdc_acm::*;
pub use crate::serial_port::*;
pub use crate::buffer::DefaultBufferStore;