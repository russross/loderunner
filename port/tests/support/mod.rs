use lode_original::devices::hires::Hires;
use lode_original::graphics::row_address;
use lode_port::collision::{Layer, Scene};

// Exact visible ink plus the uniform phase determines every decoded RGB pixel.
// Display holes are not part of the native scene or the rendered-output contract.
pub fn assert_visible(scene: &Scene, original: &Hires) {
    for layer in [Layer::Foreground, Layer::Background] {
        for row in 0..192 {
            let mut words = [0; 5];
            let address = row_address(layer.index(), row as u8);
            for byte in 0..40 {
                let pixels = original.read(address + byte as u16);
                assert!(
                    pixels & 128 != 0,
                    "color phase, layer {layer:?}, row {row}, byte {byte}"
                );
                let bits = u64::from(pixels & 127);
                let offset = byte * 7;
                words[offset / 64] |= bits << (offset % 64);
                if offset % 64 > 57 {
                    words[offset / 64 + 1] |= bits >> (64 - offset % 64);
                }
            }
            assert_eq!(
                scene.row_bits(layer, row),
                words,
                "layer {layer:?}, row {row}"
            );
        }
    }
}
