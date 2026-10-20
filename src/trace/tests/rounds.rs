use lode_trace::{
    Action, Actor, ActorId, Cell, FORMAT_VERSION, Facing, Header, Input, Life, Motion, Observer,
    Occurrence, Phase, Session, Snapshot, Subject, Tile, Trace,
};

fn initial() -> Snapshot {
    Snapshot {
        session: Session {
            level: 1,
            attempt: 0,
            phase: Phase::Playing,
            score: 0,
            lives: 5,
            exit_available: false,
        },
        actors: vec![Actor {
            id: ActorId::Player,
            cell: Cell::new(4, 5).unwrap(),
            x_adjust: 2,
            y_adjust: 2,
            facing: Facing::Right,
            motion: Motion::Standing,
            action: Action::None,
            life: Life::Alive,
            carrying_gold: false,
        }],
        active: vec![Tile::Empty; 448],
        background: vec![Tile::Empty; 448],
        hidden_ladders: vec![],
        holes: vec![],
    }
}

// Two identical accepted keys must survive pruning, and independent subjects may
// arrive in different orders without changing the serialized round.
#[test]
fn repeated_deliveries_and_independent_episodes_round_trip() {
    let before = initial();
    let mut after = before.clone();
    after.actors[0].motion = Motion::Falling;
    let inputs = vec![
        Input {
            observation: 0,
            key: 0xcc,
        },
        Input {
            observation: 1,
            key: 0xcc,
        },
    ];
    let a = (Subject::Actor(ActorId::Player), Occurrence::Landed);
    let b = (
        Subject::Cell(Cell::new(4, 6).unwrap()),
        Occurrence::Contact {
            other: Subject::Actor(ActorId::Player),
        },
    );
    let mut first = Observer::new(before.clone()).unwrap();
    let mut second = Observer::new(before.clone()).unwrap();
    let round = first
        .finish_round(0, after.clone(), inputs.clone(), vec![a.clone(), b.clone()])
        .unwrap();
    assert_eq!(
        round,
        second
            .finish_round(0, after.clone(), inputs, vec![b, a])
            .unwrap()
    );
    let trace = Trace {
        header: Header {
            version: FORMAT_VERSION,
            source: "fixture".into(),
            configuration: "keyboard".into(),
            initial: before,
        },
        rounds: vec![round],
        final_delta: 100,
        final_state: after,
    };
    assert_eq!(Trace::from_json(&trace.to_json().unwrap()).unwrap(), trace);
}

#[test]
fn rejects_tampered_change_and_final_state() {
    let before = initial();
    let mut after = before.clone();
    after.session.score = 250;
    let round = Observer::new(before.clone())
        .unwrap()
        .finish_round(0, after.clone(), vec![], vec![])
        .unwrap();
    let mut trace = Trace {
        header: Header {
            version: FORMAT_VERSION,
            source: "fixture".into(),
            configuration: "keyboard".into(),
            initial: before,
        },
        rounds: vec![round],
        final_delta: 100,
        final_state: after,
    };
    trace.final_state.session.score = 0;
    assert!(trace.validate().is_err());
    trace.final_state.session.score = 250;
    let repeated = trace.rounds[0].changes[0].clone();
    trace.rounds[0].changes.push(repeated);
    assert!(trace.validate().is_err());
}

#[test]
fn transient_episode_order_remains_significant() {
    let state = initial();
    let subject = Subject::Actor(ActorId::Player);
    let mut observer = Observer::new(state.clone()).unwrap();
    let round = observer
        .finish_round(
            0,
            state,
            vec![],
            vec![
                (subject, Occurrence::GoldCollected),
                (
                    subject,
                    Occurrence::GoldDropped {
                        cell: Cell::new(4, 5).unwrap(),
                    },
                ),
            ],
        )
        .unwrap();
    assert_eq!(round.episodes[0].events[0], Occurrence::GoldCollected);
}

#[test]
fn repeated_and_reordered_contacts_canonicalize_within_an_episode() {
    let state = initial();
    let subject = Subject::Actor(ActorId::Player);
    let contact = |col| Occurrence::Contact {
        other: Subject::Cell(Cell::new(col, 5).unwrap()),
    };
    let mut first = Observer::new(state.clone()).unwrap();
    let mut second = Observer::new(state.clone()).unwrap();
    let a = first
        .finish_round(
            0,
            state.clone(),
            vec![],
            vec![
                (subject, contact(6)),
                (subject, contact(4)),
                (subject, contact(6)),
            ],
        )
        .unwrap();
    let b = second
        .finish_round(
            0,
            state,
            vec![],
            vec![(subject, contact(4)), (subject, contact(6))],
        )
        .unwrap();
    assert_eq!(a, b);
}
