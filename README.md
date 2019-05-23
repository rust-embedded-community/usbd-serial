usbd-serial
===========

A work-in progress minimal CDC-ACM (USB serial port) class for
[usb-device](https://github.com/mvirkkunen/usb-device).

Currently it only exposes a packet-oriented interface that's efficient, but not stream-oriented like
a read serial port. Once I find a buffer implementation that I like it will also have a
stream-oriented serial port with the standard write/read interface as well as embedded-hal serial
trait compatibility.
