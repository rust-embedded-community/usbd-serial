#![no_std]

mod cdc_acm;

pub use usb_device::{Result, UsbError};
pub use crate::cdc_acm::*;
