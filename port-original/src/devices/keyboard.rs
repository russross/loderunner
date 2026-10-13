#[derive(Clone, Debug, Default)]
pub struct Keyboard {
    code: u8,
    available: bool,
}

impl Keyboard {
    pub fn deliver(&mut self, code: u8) {
        self.code = code & 0x7f;
        self.available = true;
    }

    pub fn read(&self) -> u8 {
        self.code | if self.available { 0x80 } else { 0 }
    }

    // Either a read or a write to the strobe acknowledges the code; the lower
    // seven bits remain latched and there is no physical release indication.
    pub fn clear_strobe(&mut self) {
        self.available = false;
    }

    pub fn read_mmio(&mut self, address: u16) -> u8 {
        match address {
            0xc000 => self.read(),
            0xc010 => {
                self.clear_strobe();
                self.read()
            }
            _ => panic!("unsupported keyboard address {address:04x}"),
        }
    }

    pub fn write_mmio(&mut self, address: u16, _value: u8) {
        assert_eq!(address, 0xc010);
        self.clear_strobe();
    }
}
