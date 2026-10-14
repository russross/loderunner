use lode_mame_validation::Tolerance;

#[test]
fn relative_drift_is_local_and_combines_absolute_and_percentage_limits() {
    let tolerance = Tolerance {
        cycles: 2000,
        percent: 10,
    };
    assert!(tolerance.accepts(50_000, 57_000));
    assert!(tolerance.accepts(50_000, 43_000));
    assert!(!tolerance.accepts(50_000, 57_001));
    assert!(!tolerance.accepts(50_000, 42_999));
    assert!(tolerance.accepts(0, 2000));
    assert!(!tolerance.accepts(0, 2001));
    assert!(tolerance.accepts(u64::MAX, u64::MAX));
    assert!(
        Tolerance {
            cycles: u64::MAX,
            percent: u32::MAX
        }
        .accepts(u64::MAX, 0)
    );
}
