use core::convert::Infallible;

use embassy_executor::task;
use embassy_time::{Duration, with_timeout};
use embedded_io_async::Read;
use esp_hal::{Async, usb::usb_serial_jtag::UsbSerialJtag};
use feestje_protocol::serial::{Command, Header};
use log::{info, warn};

#[task]
pub async fn usb_driver_task(mut usb: UsbSerialJtag<'static, Async>) {
    let mut buffer = [0; 256];
    // let mut state = RxState::default();
    // let mut decoder = SerialDecoder::new();

    let mut rxc = 0;

    loop {
        let header = read_header(&mut usb, &mut buffer).await;

        // Length field in header dictates the amount of payload bytes expected to be received next.
        // If this does not match the expected payload length for this command we must drain the stream.

        if header.length() != header.command().length() {
            warn!(
                "Payload length doesn't match expected length (expected: {}, got: {})",
                header.command().length(),
                header.length()
            );

            // Drain buffer
            drain(&mut usb, &mut buffer, header.length()).await;
            continue;
        }

        info!("Header = {header:?}");

        rxc += 1;

        match header.command() {
            Command::Demo4 => panic!("Well shit"),
            _ => info!("Do something: {rxc}"),
        }
    }
}

async fn read_header<R: Read<Error = Infallible>>(read: &mut R, buffer: &mut [u8]) -> Header {
    loop {
        if read.read_exact(&mut buffer[..5]).await.is_err() {
            continue;
        }

        if let Some(header) = Header::decode(buffer) {
            return header;
        }

        warn!("Invalid header received");

        // Empty any potential remaining bytes in the buffer
        // (No clue if this would even help with that but hey I try)
        // (Oh and in the case there's *a lot* of buffered data I just hope this method loops until it's empty)
        _ = with_timeout(Duration::from_millis(10), read.read_exact(buffer)).await;
    }
}

/// Drain a set amount of bytes from a readable stream
async fn drain<R: Read<Error = Infallible>>(read: &mut R, buffer: &mut [u8], length: u16) {
    let mut drained = 0;
    while drained < length as usize {
        let Ok(len) = read.read(buffer).await;
        drained += len;
    }
}

// struct SerialDecoder {
//     state: RxState,
// }

// impl SerialDecoder {
//     fn new() -> Self {
//         Self {
//             state: RxState::Header {
//                 buf: [0; HEADER_SIZE],
//                 received: 0,
//             },
//         }
//     }

//     fn feed(&mut self, mut data: &[u8]) {
//         while !data.is_empty() {
//             let mut new_state = None;

//             match &mut self.state {
//                 RxState::Header { buf, received } => {
//                     let remaining = HEADER_SIZE - *received;
//                     let n = remaining.min(data.len());

//                     buf[*received..*received + n].copy_from_slice(&data[..n]);

//                     *received += n;
//                     data = &data[n..];

//                     if *received == HEADER_SIZE {
//                         let Some(header) = Header::decode(&buf) else {
//                             warn!("Received invalid header");
//                             *received = 0;
//                             continue;
//                         };

//                         new_state = Some(RxState::Payload {
//                             command: header.command(),
//                             received: 0,
//                         });
//                     }
//                 }

//                 RxState::Payload { command, received } => {
//                     debug!("idk yet");
//                 }
//             }

//             if let Some(state) = new_state {
//                 self.state = state;
//             }
//         }
//     }
// }

// enum RxState {
//     Header {
//         buf: [u8; HEADER_SIZE],
//         received: usize,
//     },

//     Payload {
//         command: Command,
//         received: usize,
//     },
// }

// impl Default for RxState {
//     fn default() -> Self {
//         Self::Header {
//             buf: [0; HEADER_SIZE],
//             received: 0,
//         }
//     }
// }
