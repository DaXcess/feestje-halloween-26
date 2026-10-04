#[repr(C, packed)]
#[derive(Debug)]
pub struct Header {
    magic: u16,
    command: Command,
    length: u16,
}

impl Header {
    pub const MAGIC: u16 = 0xAB55;
    pub const SIZE: usize = core::mem::size_of::<Self>();

    pub fn new(command: Command, length: u16) -> Self {
        Self {
            magic: Self::MAGIC,
            command,
            length,
        }
    }

    pub fn decode(bytes: &[u8]) -> Option<Self> {
        let magic = u16::from_le_bytes(bytes[..2].try_into().ok()?);
        if magic != Self::MAGIC {
            return None;
        }

        let command = Command::from_byte(bytes[2])?;
        let length = u16::from_le_bytes(bytes[3..5].try_into().ok()?);

        Some(Self {
            magic,
            command,
            length,
        })
    }

    pub fn command(&self) -> Command {
        self.command
    }

    pub fn length(&self) -> u16 {
        self.length
    }

    pub fn encode(&self) -> [u8; Self::SIZE] {
        let mut buf = [0; Self::SIZE];
        buf[0..2].copy_from_slice(&Self::MAGIC.to_le_bytes());
        buf[2] = self.command as u8;
        buf[3..].copy_from_slice(&self.length.to_le_bytes());

        buf
    }
}

#[repr(u8)]
#[derive(Clone, Copy, Debug)]
pub enum Command {
    Demo1,
    Demo2,
    Demo3,
    Demo4,
}

impl Command {
    pub fn length(&self) -> u16 {
        match self {
            Self::Demo1 | Self::Demo2 | Self::Demo3 | Self::Demo4 => 0,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Self::Demo1,
            1 => Self::Demo2,
            2 => Self::Demo3,
            3 => Self::Demo4,
            _ => None?,
        })
    }
}
