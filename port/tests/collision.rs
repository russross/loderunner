use lode_original::devices::hires::Hires;
use lode_original::graphics::{
    compute_shifted_sprite, draw_sprite, draw_sprite_at_pixels, erase_sprite_at_pixels, row_address,
};
use lode_original::session::{Configuration as OriginalConfiguration, Run as OriginalRun};
use lode_port::actors::GuardMode;
use lode_port::collision::{Layer, Scene, Shape};
use lode_port::session::{Configuration, Run};
use lode_port::state::Game;
use lode_port::storage::{LevelRecord, original_level};
use lode_trace::Phase;

mod support;

const PLAYERS: [u8; 18] = [
    11, 12, 13, 24, 25, 26, 15, 19, 9, 16, 17, 21, 22, 23, 37, 20, 14, 18,
];
const GUARDS: [u8; 16] = [
    8, 43, 44, 48, 49, 50, 54, 40, 41, 42, 45, 46, 47, 53, 51, 52,
];

fn rows(sprite: u8) -> [u32; 11] {
    let block = compute_shifted_sprite(sprite, 0);
    std::array::from_fn(|row| {
        u32::from(block[row * 3] & 127) | u32::from(block[row * 3 + 1] & 127) << 7
    })
}

// An independent row predicate covers every close animation pair, including
// negative translations and contacts crossing the top, bottom and side edges.
#[test]
fn whole_shapes_match_all_animation_pair_intersections() {
    for player in PLAYERS {
        let a = rows(player);
        for guard in GUARDS {
            let b = rows(guard);
            for dx in -9i16..=9 {
                for dy in -10i16..=10 {
                    let expected = (0i16..11).any(|y| {
                        let by = y - dy;
                        (0..11).contains(&by)
                            && a[y as usize]
                                & if dx >= 0 {
                                    b[by as usize] << dx
                                } else {
                                    b[by as usize] >> -dx
                                }
                                != 0
                    });
                    assert_eq!(
                        Shape::sprite(player).intersects(Shape::sprite(guard).translated(dx, dy)),
                        expected,
                        "player {player}, guard {guard}, offset {dx},{dy}"
                    );
                }
            }
        }
    }
}

struct Reference {
    scene: Scene,
    hires: Hires,
}

impl Reference {
    fn new() -> Self {
        Self {
            scene: Scene::default(),
            hires: Hires::default(),
        }
    }
    fn tile(&mut self, page: usize, sprite: u8, col: u8, row: u8) {
        self.scene.tile(
            if page == 0 {
                Layer::Foreground
            } else {
                Layer::Background
            },
            sprite,
            col,
            row,
        );
        draw_sprite(&mut self.hires, page, sprite, col, row);
    }
    fn overlay(&mut self, sprite: u8, x: u8, y: u8) {
        self.scene.overlay(sprite, x, y);
        draw_sprite_at_pixels(&mut self.hires, sprite, x, y);
    }
    fn erase(&mut self, sprite: u8, x: u8, y: u8) {
        self.scene.erase(sprite, x, y);
        erase_sprite_at_pixels(&mut self.hires, sprite, x, y);
    }
    fn probe(&mut self, sprite: u8, x: u8, y: u8) -> bool {
        let actual = self.scene.contact(sprite, x, y);
        let expected = draw_sprite_at_pixels(&mut self.hires, sprite, x, y) != 0;
        self.scene.overlay(sprite, x, y);
        assert_eq!(actual, expected, "sprite {sprite}, half_x {x}, y {y}");
        actual
    }
}

// A background replacement can create negative contrast. Empty-sprite erasure
// restores it across its full span, including the neighboring cell's ink.
#[test]
fn restoration_clears_deficits_beyond_the_erased_shape() {
    let mut reference = Reference::new();
    reference.tile(1, 1, 6, 5);
    assert!(reference.probe(9, 30, 55));
    reference.tile(0, 0, 6, 5);
    reference.erase(0, 28, 55);
    assert!(!reference.probe(9, 30, 55));
}

// Both geometric contacts and their suppression by prior player erasure are
// explicit. No per-actor pixel ownership is needed to reproduce that sequence.
#[test]
fn old_player_erasure_can_remove_the_next_contact_pixel() {
    let mut reference = Reference::new();
    reference.overlay(8, 50, 55);
    reference.overlay(9, 49, 45);
    assert!(reference.scene.contact(16, 50, 45));
    reference.erase(9, 49, 45);
    assert!(!reference.probe(16, 50, 45));
}

#[test]
fn synchronized_background_masks_ink_and_replacement_exposes_it() {
    let mut reference = Reference::new();
    reference.tile(1, 8, 10, 5);
    reference.tile(0, 8, 10, 5);
    assert!(!reference.probe(9, 50, 55));
    reference.tile(1, 0, 10, 5);
    assert!(reference.probe(9, 50, 55));
}

#[test]
fn right_edge_ink_and_erasure_preserve_the_lower_left_contact() {
    let mut reference = Reference::new();
    reference.overlay(37, 136, 55);
    assert!(reference.probe(11, 0, 119));
    reference.erase(11, 0, 119);
    reference.erase(37, 136, 55);
    assert!(!reference.probe(11, 0, 119));
}

#[test]
fn status_text_uses_its_actual_vertical_position() {
    let mut reference = Reference::new();
    reference.tile(0, 69, 10, 16);
    assert!(reference.probe(9, 50, 181));
    reference.tile(0, 0, 10, 16);
    assert!(!reference.probe(9, 50, 181));
}

// Exercise arbitrary mixtures, rather than only well-behaved scene unions.
// Probes use visible player bounds; overlay/erase also exercise right-edge spill.
#[test]
fn mixed_shape_operations_match_the_framebuffer_detector() {
    let mut reference = Reference::new();
    let mut random = 0x4d595df4d0f33173u64;
    for step in 0..30_000 {
        random ^= random << 13;
        random ^= random >> 7;
        random ^= random << 17;
        let sprite = (random % 102) as u8;
        let x = ((random >> 8) % 140) as u8;
        let y = ((random >> 16) % 182) as u8;
        match random % 6 {
            0 | 1 => reference.tile((random % 6) as usize, sprite, x % 28, y % 17),
            2 => reference.overlay(sprite, x, y),
            3 => reference.erase(sprite, x, y),
            4 => {
                reference.scene.clear_background();
                reference.hires.clear(1);
            }
            _ => {
                reference.scene.status_border();
                for row in 176..180 {
                    for col in 0..40 {
                        reference.hires.write(
                            row_address(0, row) + col,
                            if col % 2 == 0 { 0xd5 } else { 0xaa },
                        );
                    }
                }
            }
        }
        let player = PLAYERS[((random >> 24) as usize) % PLAYERS.len()];
        reference.probe(player, x.min(135), y);
        if step % 100 == 0 {
            for shift in 0..7 {
                reference.probe(player, 49 + shift, 55);
            }
        }
    }
}

fn framebuffer_contact(hires: &Hires, sprite: u8, x: u8, y: u8) -> bool {
    let shift = (u16::from(x) * 2 % 7) as u8;
    let block = compute_shifted_sprite(sprite, shift);
    for (row, pixels) in block.as_chunks::<3>().0.iter().enumerate() {
        let address = row_address(0, y + row as u8) + u16::from(x) * 2 / 7;
        for (column, bits) in pixels
            .iter()
            .take(if shift >= 5 { 3 } else { 2 })
            .enumerate()
        {
            let address = address + column as u16;
            if (hires.read(address) ^ hires.read(address + 0x2000)) & bits != 0 {
                return true;
            }
        }
    }
    false
}

fn intact_guard_contact(game: &Game, sprite: u8, x: u8, y: u8, terrain: bool) -> bool {
    let player = rows(sprite);
    game.guards.iter().any(|guard| {
        if matches!(guard.mode, GuardMode::Resurrecting { .. }) {
            return false;
        }
        let (gx, gy) = guard.position.half_x_and_pixel_y();
        let (dx, dy) = (
            (i16::from(gx) - i16::from(x)) * 2,
            i16::from(gy) - i16::from(y),
        );
        if dx.abs() >= 10 || dy.abs() >= 11 {
            return false;
        }
        let shape = rows(guard.animation.sprite());
        (0i16..11).any(|row| {
            let guard_row = row - dy;
            if !(0..11).contains(&guard_row) {
                return false;
            }
            let mut overlap = player[row as usize]
                & if dx >= 0 {
                    shape[guard_row as usize] << dx
                } else {
                    shape[guard_row as usize] >> -dx
                };
            if terrain {
                let background = game
                    .scene
                    .row_bits(Layer::Background, usize::from(y) + row as usize);
                for column in 0..10 {
                    let px = u16::from(x) * 2 + column;
                    if background[usize::from(px) / 64] & (1 << (px % 64)) != 0 {
                        overlap &= !(1 << column);
                    }
                }
            }
            overlap != 0
        })
    })
}

// Forced probes of actually reached states locate pose-only disagreements. They
// do not imply that each proposed pose occurs in an unmodified gameplay round.
#[test]
fn forced_probes_from_reached_states_match_scene_while_pose_only_does_not() {
    let mut probes = 0;
    let mut mismatches = 0;
    let mut terrain_mismatches = 0;
    let mut false_positives = 0;
    let mut false_negatives = 0;
    let old_model = support::reference_model();
    for level in 1..=150 {
        let config = Configuration {
            levels: vec![LevelRecord(original_level(level).unwrap())],
            start: 0,
            high_scores: vec![],
            wait_on_level_start: false,
        };
        let old_config: OriginalConfiguration =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        let mut old = OriginalRun::new(old_config, false).unwrap();
        let mut run = Run::new(config, false).unwrap();
        for turn in 0..180 {
            // Inspect the proposed new player after erasing its old shape. The
            // scenario is a forced contact experiment, not a committed game turn.
            let (sprite, x, y) = run.game.player_sprite_coordinates();
            let mut scene = run.game.scene.clone();
            scene.erase(sprite, x, y);
            let mut probe = Reference::new();
            support::assert_visible(&run.game.scene, &old.game.hires);
            for address in 0x2000..0x6000 {
                let expected = old.game.hires.read(address);
                probe.hires.write(address, expected);
            }
            erase_sprite_at_pixels(&mut probe.hires, sprite, x, y);
            for candidate in [x.saturating_sub(1), x, (x + 1).min(135)] {
                let actual = framebuffer_contact(&probe.hires, sprite, candidate, y);
                assert_eq!(
                    scene.contact(sprite, candidate, y),
                    actual,
                    "level {level}, turn {turn}, candidate {candidate}"
                );
                let pose_only = intact_guard_contact(&run.game, sprite, candidate, y, false);
                let masked = intact_guard_contact(&run.game, sprite, candidate, y, true);
                probes += 1;
                mismatches += usize::from(actual != pose_only);
                terrain_mismatches += usize::from(actual != masked);
                false_positives += usize::from(masked && !actual);
                false_negatives += usize::from(actual && !masked);
            }
            let key = Some([0xcc, 0xca, 0xc9, 0xcb, 0xcf, 0xd5, 0xa0][turn / 15 % 7]);
            old.step(key).unwrap();
            run.step(key).unwrap();
            assert_eq!(
                run.game.snapshot(),
                old.game.snapshot(),
                "level {level}, turn {turn}"
            );
            let interval = old.last_timing().unwrap();
            if interval.phase == Phase::Playing
                && matches!(
                    interval.end_phase,
                    Phase::Playing | Phase::Died | Phase::LevelCleared
                )
            {
                assert_eq!(
                    run.last_model_cycles().unwrap(),
                    old_model.estimate(interval)
                );
            }
        }
        let expected = old.trace().unwrap();
        let mut actual = run.trace().unwrap();
        for (actual, expected) in actual.rounds.iter_mut().zip(&expected.rounds) {
            actual.start_delta = expected.start_delta;
            assert_eq!(actual, expected, "level {level}: logical round");
        }
    }
    println!(
        "forced probes from reached states: {probes}, pose-only mismatches: {mismatches}, terrain-masked mismatches: {terrain_mismatches} ({false_positives} false positives, {false_negatives} false negatives)"
    );
    assert!(mismatches > 0);
}
