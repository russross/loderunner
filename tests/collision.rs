use loderunner::collision::{Layer, Scene};

// Restoring a span clears contrast outside the erased sprite's own ink.
#[test]
fn restoration_clears_deficits_beyond_the_erased_shape() {
    let mut scene = Scene::default();
    scene.tile(Layer::Background, 1, 6, 5);
    assert!(scene.contact(9, 30, 55));
    scene.overlay(9, 30, 55);
    scene.tile(Layer::Foreground, 0, 6, 5);
    scene.erase(0, 28, 55);
    assert!(!scene.contact(9, 30, 55));
}

#[test]
fn old_player_erasure_can_remove_the_next_contact_pixel() {
    let mut scene = Scene::default();
    scene.overlay(8, 50, 55);
    scene.overlay(9, 49, 45);
    assert!(scene.contact(16, 50, 45));
    scene.erase(9, 49, 45);
    assert!(!scene.contact(16, 50, 45));
}

// Matching background ink suppresses contact until background replacement.
#[test]
fn synchronized_background_masks_ink_and_replacement_exposes_it() {
    let mut scene = Scene::default();
    scene.tile(Layer::Background, 8, 10, 5);
    scene.tile(Layer::Foreground, 8, 10, 5);
    assert!(!scene.contact(9, 50, 55));
    scene.overlay(9, 50, 55);
    scene.tile(Layer::Background, 0, 10, 5);
    assert!(scene.contact(9, 50, 55));
}

#[test]
fn right_edge_ink_and_erasure_preserve_the_lower_left_contact() {
    let mut scene = Scene::default();
    scene.overlay(37, 136, 55);
    assert!(scene.contact(11, 0, 119));
    scene.overlay(11, 0, 119);
    scene.erase(11, 0, 119);
    scene.erase(37, 136, 55);
    assert!(!scene.contact(11, 0, 119));
}

#[test]
fn status_text_uses_its_actual_vertical_position() {
    let mut scene = Scene::default();
    scene.tile(Layer::Foreground, 69, 10, 16);
    assert!(scene.contact(9, 50, 181));
    scene.overlay(9, 50, 181);
    scene.tile(Layer::Foreground, 0, 10, 16);
    assert!(!scene.contact(9, 50, 181));
}
