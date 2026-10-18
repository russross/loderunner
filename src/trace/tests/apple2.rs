use lode_trace::apple2::{Apple2Observer, Sample, Signal};
use lode_trace::{
    Action, ActorId, Cell, Facing, HolePhase, Life, Motion, Occurrence, Phase, Subject,
};
use std::fmt::Write;

fn sample(memory: &[u8; 8192], signals: Vec<Signal>, attempt: u32) -> Sample {
    let mut hex = String::with_capacity(16384);
    for byte in memory {
        write!(hex, "{byte:02x}").unwrap();
    }
    Sample {
        cycle: 0,
        phase: Phase::Playing,
        attempt,
        memory: hex,
        inputs: vec![],
        signals,
    }
}

fn memory() -> [u8; 8192] {
    let mut bytes = [0; 8192];
    bytes[0] = 5;
    bytes[1] = 15;
    bytes[2] = 2;
    bytes[3] = 2;
    bytes[0x9a] = 1;
    bytes[0xa6] = 1;
    bytes[0x98] = 5;
    bytes
}

// The decoder consumes machine storage and executed-path signals. It neither
// reconstructs expected changes nor moves objects using native gameplay rules.
#[test]
fn packed_storage_and_object_flags_decode_independently() {
    let mut bytes = memory();
    bytes[0x8e..0x92].copy_from_slice(&[0x56, 0x34, 0x12, 0]);
    bytes[0x8d] = 1;
    bytes[0xc61] = 7;
    bytes[0xc69] = 8;
    bytes[0xc71] = 0xff;
    bytes[0xc79] = 3;
    bytes[0xc81] = 1;
    bytes[0xc91] = 0xff;
    bytes[0x1f00..0x1f08].copy_from_slice(&[0xc1, 0xc2, 0xc3, 9, 0, 0x12, 0x34, 0x56]);
    let mut decoder = Apple2Observer::new(1).unwrap();
    let observed = decoder
        .observe(&sample(
            &bytes,
            vec![Signal::Motion {
                actor: ActorId::Guard(1),
                motion: Motion::Left,
            }],
            0,
        ))
        .unwrap();
    assert_eq!(observed.snapshot.session.score, 123456);
    assert_eq!(observed.snapshot.high_scores[0].score, 123456);
    assert_eq!(observed.snapshot.high_scores[0].initials, *b"ABC");
    let guard = &observed.snapshot.actors[1];
    assert_eq!(guard.facing, Facing::Left);
    assert!(guard.carrying_gold);
    assert_eq!(guard.action, Action::None);
    assert_eq!(guard.life, Life::Alive);
    assert_eq!(guard.motion, Motion::Left);
}

#[test]
fn hole_generation_and_motion_reset_follow_attempt_boundaries() {
    let mut bytes = memory();
    bytes[0xca0] = 4;
    bytes[0xcc0] = 15;
    bytes[0xce0] = 180;
    let mut decoder = Apple2Observer::new(0).unwrap();
    let first = decoder
        .observe(&sample(
            &bytes,
            vec![Signal::Motion {
                actor: ActorId::Player,
                motion: Motion::Falling,
            }],
            0,
        ))
        .unwrap();
    assert_eq!(first.snapshot.holes[0].generation, 1);
    bytes[0xce0] = 20;
    assert_eq!(
        decoder
            .observe(&sample(&bytes, vec![], 0))
            .unwrap()
            .snapshot
            .holes[0]
            .phase,
        HolePhase::ClosingFirst
    );
    bytes[0xce0] = 10;
    assert_eq!(
        decoder
            .observe(&sample(&bytes, vec![], 0))
            .unwrap()
            .snapshot
            .holes[0]
            .phase,
        HolePhase::ClosingSecond
    );
    bytes[0xce0] = 0;
    decoder.observe(&sample(&bytes, vec![], 0)).unwrap();
    bytes[0xce0] = 180;
    assert_eq!(
        decoder
            .observe(&sample(&bytes, vec![], 0))
            .unwrap()
            .snapshot
            .holes[0]
            .generation,
        2
    );
    let restarted = decoder.observe(&sample(&bytes, vec![], 1)).unwrap();
    assert_eq!(restarted.snapshot.holes[0].generation, 1);
    assert_eq!(restarted.snapshot.actors[0].motion, Motion::Standing);
}

#[test]
fn transient_occurrences_survive_identical_snapshots() {
    let mut decoder = Apple2Observer::new(0).unwrap();
    let bytes = memory();
    let initial = decoder
        .observe(&sample(&bytes, vec![], 0))
        .unwrap()
        .snapshot;
    let cell = Cell::new(6, 15).unwrap();
    let observed = decoder
        .observe(&sample(
            &bytes,
            vec![
                Signal::Collected {
                    actor: ActorId::Guard(1),
                },
                Signal::Dropped { slot: 1, cell },
            ],
            0,
        ))
        .unwrap();
    assert_eq!(initial, observed.snapshot);
    assert_eq!(
        observed.occurrences,
        vec![
            (Subject::Actor(ActorId::Guard(1)), Occurrence::GoldCollected),
            (
                Subject::Actor(ActorId::Guard(1)),
                Occurrence::GoldDropped { cell }
            )
        ]
    );
}

#[test]
fn malformed_memory_bcd_and_signal_domains_are_rejected() {
    let mut decoder = Apple2Observer::new(0).unwrap();
    let mut invalid = sample(&memory(), vec![], 0);
    invalid.memory.pop();
    assert!(decoder.observe(&invalid).is_err());
    let mut bytes = memory();
    bytes[0x8e] = 0xfa;
    assert!(decoder.observe(&sample(&bytes, vec![], 0)).is_err());
    assert!(
        decoder
            .observe(&sample(&memory(), vec![Signal::SelectGuard { slot: 0 }], 0))
            .is_err()
    );
    assert!(
        decoder
            .observe(&sample(
                &memory(),
                vec![Signal::Contact {
                    actor: ActorId::Player,
                    other: Subject::Cell(Cell { col: 28, row: 0 })
                }],
                0
            ))
            .is_err()
    );
}
