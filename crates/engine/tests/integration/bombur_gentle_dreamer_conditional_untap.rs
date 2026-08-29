//! Bombur, Gentle Dreamer (HOB): "Storied (If you control three or more
//! artifacts, legendaries, and/or Sagas, you have an enduring story for the
//! rest of the game.) Bombur doesn't untap during your untap step unless you
//! have an enduring story."
//!
//! CR 502.3 (an effect can keep a permanent from untapping during the untap
//! step) + CR 702.195a-b (Storied grants the "enduring story" designation once
//! its controller controls the Storied permanent plus three or more historic
//! permanents; the designation persists for the rest of the game). "Unless" is
//! a negative-polarity conditional gate — the restriction applies precisely
//! when the trailing condition is false — and per CR 611.3a that condition is
//! re-evaluated dynamically at every untap step rather than "locked in" once.
//!
//! Two-sided regression: "doesn't untap ... unless [condition]" is a negative
//! conditional — the restriction (staying tapped) is the DEFAULT, and the
//! "unless" clause is the exception that lifts it. So without an enduring
//! story Bombur stays tapped through its controller's own untap step; with
//! one, it untaps like any other permanent. Both cases are driven through the
//! real turn-structure production path (`GameRunner::advance_to_phase`), not
//! a direct call into the untap-step internals.

use engine::game::scenario::{GameScenario, P1};
use engine::types::phase::Phase;

const BOMBUR: &str = "Storied (If you control three or more artifacts, legendaries, and/or Sagas, you have an enduring story for the rest of the game.)\nBombur doesn't untap during your untap step unless you have an enduring story.";

#[test]
fn bombur_stays_tapped_without_enduring_story() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    // P1 controls only Bombur itself: one historic permanent (legendary), short
    // of Storied's three-or-more threshold, so no enduring story is granted.
    let bombur = scenario
        .add_creature_from_oracle(P1, "Bombur, Gentle Dreamer", 5, 3, BOMBUR)
        .as_legendary()
        .id();

    let mut runner = scenario.build();
    runner.state_mut().objects.get_mut(&bombur).unwrap().tapped = true;

    // Advance past P0's remaining phases and into P1's own untap step (CR 502.3),
    // stopping at the next Upkeep priority window.
    runner.advance_to_phase(Phase::Upkeep);
    assert_eq!(
        runner.state().active_player,
        P1,
        "should now be P1's turn (their untap step has processed)"
    );

    // Reach guard: confirm the premise this test hinges on before trusting the
    // negative assertion below.
    assert!(
        !runner.state().enduring_story.contains(&P1),
        "reach guard: P1 must NOT have an enduring story with only one historic permanent"
    );
    assert!(
        runner.state().objects[&bombur].tapped,
        "without an enduring story, Bombur must stay tapped through its controller's untap step"
    );
}

#[test]
fn bombur_untaps_normally_with_enduring_story() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    let bombur = scenario
        .add_creature_from_oracle(P1, "Bombur, Gentle Dreamer", 5, 3, BOMBUR)
        .as_legendary()
        .id();
    // Two more legendary permanents so P1 controls three historic permanents
    // (Bombur itself plus these two) alongside the Storied permanent (Bombur),
    // satisfying CR 702.195a's enduring-story grant well before P1's own untap
    // step is reached (state-based actions run on every intervening priority
    // pass while the turn structure advances).
    scenario
        .add_creature(P1, "Legendary Friend One", 1, 1)
        .as_legendary();
    scenario
        .add_creature(P1, "Legendary Friend Two", 1, 1)
        .as_legendary();

    let mut runner = scenario.build();
    runner.state_mut().objects.get_mut(&bombur).unwrap().tapped = true;

    runner.advance_to_phase(Phase::Upkeep);
    assert_eq!(
        runner.state().active_player,
        P1,
        "should now be P1's turn (their untap step has processed)"
    );

    // Reach guard: confirm the premise (enduring story actually granted) before
    // trusting the "untaps normally" assertion — otherwise a broken CantUntap
    // condition and a broken enduring-story grant could both silently pass.
    assert!(
        runner.state().enduring_story.contains(&P1),
        "reach guard: P1 must have gained an enduring story from controlling three historic permanents"
    );
    assert!(
        !runner.state().objects[&bombur].tapped,
        "with an enduring story, Bombur must untap normally during its controller's untap step"
    );
}
