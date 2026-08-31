//! Issue #8162 — Varragoth, Bloodsky Sire's Boast puts the searched card into
//! hand instead of on top of the library.
//!
//! Oracle text (verified against Scryfall): "Deathtouch\nBoast — {1}{B}: Target
//! player searches their library for a card, then shuffles and puts that card
//! on top. (Activate only if this creature attacked this turn and only once
//! each turn.)"
//!
//! CR 701.24b: "Some effects cause a player to search a library for a card or
//! cards, shuffle that library, then put some or all of the found cards into a
//! different zone or in a certain position in that library." Varragoth's Boast
//! is exactly this shape — the found card must stay in the library
//! (repositioned to the top), never routed through the hand.
//!
//! Root cause: the search's subject is third-person ("Target player ...
//! shuffles and puts that card on top") rather than "you", so the clause reads
//! "puts" (not "put"). The `has_positional_put` suppression gate in
//! `parse_intrinsic_continuation_ast` (oracle_effect/sequence.rs) only matched
//! the "put" conjugation, so it failed to suppress the default
//! `ChangeZone(Library -> Hand)` continuation, and the chain became
//! `SearchLibrary -> ChangeZone(Hand) -> Shuffle -> PutAtLibraryPosition(Top)`
//! instead of `SearchLibrary -> Shuffle -> PutAtLibraryPosition(Top)`.
//!
//! Same-class cards affected by the identical "puts that card on top"
//! conjugation: Scheming Symmetry, Deceptive Divination.
//!
//! Revert probe: reverting `parse_search_result_put_on_top_restatement` (or the
//! `has_positional_put` call site) to only recognize the bare "put" conjugation
//! reintroduces the spurious `ChangeZone(Hand)` sub-ability, which flips
//! `varragoth_boast_puts_found_card_on_top_of_library` below (the found card
//! ends up in the searching player's hand instead of on top of their library).

use engine::game::scenario::{GameRunner, GameScenario};
use engine::types::ability::Effect;
use engine::types::game_state::WaitingFor;
use engine::types::identifiers::ObjectId;
use engine::types::mana::{ManaType, ManaUnit};
use engine::types::phase::Phase;
use engine::types::player::PlayerId;
use engine::types::zones::Zone;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);

const VARRAGOTH: &str = "Deathtouch\nBoast — {1}{B}: Target player searches their library for a card, then shuffles and puts that card on top. (Activate only if this creature attacked this turn and only once each turn.)";

/// Index of Varragoth's Boast activated ability (the only `SearchLibrary`
/// ability it carries).
fn boast_index(runner: &GameRunner, varragoth: ObjectId) -> usize {
    runner.state().objects[&varragoth]
        .abilities
        .iter()
        .position(|a| matches!(a.effect.as_ref(), Effect::SearchLibrary { .. }))
        .expect("Varragoth must carry a SearchLibrary (Boast) activated ability")
}

/// Build a battlefield Varragoth (controlled by P0) that has attacked this
/// turn, with P0 holding priority on an empty stack in a main phase, enough
/// mana ({1}{B}) pooled to pay the Boast cost, and a single deterministic card
/// on top of P1's library for the mandatory search to find.
fn setup() -> (GameRunner, ObjectId, ObjectId) {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PostCombatMain);
    let varragoth = scenario
        .add_creature_from_oracle(P0, "Varragoth, Bloodsky Sire", 2, 3, VARRAGOTH)
        .id();
    scenario.with_mana_pool(
        P0,
        vec![
            ManaUnit::new(ManaType::Black, ObjectId(0), false, vec![]),
            ManaUnit::new(ManaType::Colorless, ObjectId(0), false, vec![]),
        ],
    );
    // A single card in P1's library — deterministic "search for a card" find.
    let tutored = scenario.add_card_to_library_top(P1, "Tutored Card");

    let mut runner = scenario.build();
    // CR 702.142a: Boast requires the source to have attacked this turn.
    runner
        .state_mut()
        .creatures_attacked_this_turn
        .insert(varragoth);
    runner.state_mut().active_player = P0;
    runner.state_mut().priority_player = P0;
    runner.state_mut().waiting_for = WaitingFor::Priority { player: P0 };
    (runner, varragoth, tutored)
}

/// Activating Varragoth's Boast targeting an opponent, then searching that
/// opponent's library, must leave the found card ON TOP of that opponent's
/// library (CR 701.24b) — never in their hand.
///
/// Revert probe: reverting the `has_positional_put` fix in
/// `parse_intrinsic_continuation_ast` to only match "put" (not "puts") makes
/// this fail: the found card ends up in P1's hand instead of on top of P1's
/// library.
#[test]
fn varragoth_boast_puts_found_card_on_top_of_library() {
    let (mut runner, varragoth, tutored) = setup();
    let idx = boast_index(&runner, varragoth);

    let outcome = runner
        .activate(varragoth, idx)
        .target_player(P1)
        .search_first_legal()
        .resolve();

    // The found card must still be in the library, not the hand.
    outcome.assert_zone(&[tutored], Zone::Library);

    // And specifically on TOP of P1's library (CR 701.24b: the shuffle
    // excludes the found card, which is then placed in the stated position).
    let p1 = outcome
        .state()
        .players
        .iter()
        .find(|p| p.id == P1)
        .expect("P1 exists");
    assert_eq!(
        p1.library.front(),
        Some(&tutored),
        "the searched card must be on top of P1's library after the shuffle, \
         got library order {:?}",
        p1.library
    );

    // P1's hand must be empty — the bug routed the found card there instead.
    assert!(
        p1.hand.is_empty(),
        "the searched card must NOT be delivered to P1's hand, got hand {:?}",
        p1.hand
    );
}
