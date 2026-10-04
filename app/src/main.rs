use std::{
    io::{self, ErrorKind},
    time::Duration,
};

use feestje_protocol::serial::{Command, Header};
use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf};
use tokio_serial::{
    SerialPort, SerialPortBuilder, SerialPortInfo, SerialPortType, SerialStream, UsbPortInfo,
};
use tokio_util::{
    bytes::BytesMut,
    codec::{Decoder, FramedRead},
};
use tracing::{debug, error, trace, warn};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| concat!(env!("CARGO_CRATE_NAME"), "=trace").into()),
        )
        .with_ansi_sanitization(false)
        .init();

    let ports = available_esp_devices();

    let mut device = None;
    for port in ports {
        trace!("Attempting to open device {}", port.port_name);

        let Ok(dev) = SerialStream::open(&tokio_serial::new(&port.port_name, 115200)) else {
            warn!("Could not open device {}", port.port_name);
            continue;
        };

        device = Some(dev);
        break;
    }

    let Some(device) = device else {
        error!("No available device was found");
        return;
    };

    let (serial_rx, mut serial_tx) = tokio::io::split(device);

    tokio::spawn(serial_read(serial_rx));

    trace!("Sup fuckers");

    loop {
        serial_tx
            .write_all(&Header::new(Command::Demo4, 0).encode())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Returns a list of ESP32 devices currently connected to the machine.
///
/// It is not guaranteed that these ports are actually capable of being opened.
fn available_esp_devices() -> Vec<SerialPortInfo> {
    const DEVICE_VID: u16 = 0x303a;
    const DEVICE_PID: u16 = 0x1001;

    tokio_serial::available_ports()
        .unwrap_or_default()
        .into_iter()
        .filter(|info| {
            matches!(
                info.port_type,
                SerialPortType::UsbPort(UsbPortInfo {
                    vid: DEVICE_VID,
                    pid: DEVICE_PID,
                    ..
                })
            )
        })
        .collect()
}

struct LineCodec;

impl Decoder for LineCodec {
    type Item = String;
    type Error = io::Error;

    fn decode(&mut self, src: &mut BytesMut) -> Result<Option<Self::Item>, Self::Error> {
        let newline = src.as_ref().iter().position(|b| *b == b'\n');
        if let Some(n) = newline {
            let line = src.split_to(n + 1);
            return match str::from_utf8(line.as_ref()) {
                Ok(s) => Ok(Some(s.trim_end().to_string())),
                Err(_) => Err(io::Error::new(ErrorKind::Other, "Invalid String")),
            };
        }
        Ok(None)
    }
}

async fn serial_read(rx: ReadHalf<SerialStream>) {
    let mut reader = FramedRead::new(rx, LineCodec);

    while let Some(line) = reader.next().await {
        if let Ok(line) = line {
            debug!("ESP: {line}");
        }
    }

    error!("Serial port closed");
}
