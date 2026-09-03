//! Regression coverage for the reflexive **"Surveil N. If you put a [filter]
//! card into your graveyard this way, put that card into your hand."** class
//! (Chandra, Chill of Compliance's first `[+1]`; Enlightened Confidant shares
//! the shape with a dynamic mana-value filter instead of a fixed type filter).
//!
//! This is the SURVEIL sibling of the discard/sacrifice "this way" class
//! covered by `reflexive_discard_this_way.rs` (Silvan Reveler, issue #8122):
//! a preceding "surveil N" instruction creates a reflexive gate on whichever
//! looked-at card was put into the graveyard, and the follow-up's bare
//! pronoun ("that card") must resolve to that SPECIFIC card, not to a generic
//! parent-target fallback — Effect::Surveil declares no card-shaped target of
//! its own (its `target` field is WHO surveils, a player), so the bare
//! pronoun defaults to `TargetFilter::ParentTarget`, which names nothing.
//!
//! Before this fix: (1) the condition itself did not parse at all — no
//! existing "this way" combinator recognized the active-voice "you put
//! [filter] into your graveyard this way" shape (only the passive "is put
//! into a graveyard this way" and the hand-destination active voice
//! existed) — so the whole clause fell through to
//! `Effect::Unimplemented { name: "nonland", .. }`; and (2) even with the
//! condition parsed, "that card" would resolve to `ParentTarget`, which
//! `Effect::Surveil` never populates, so the qualifying card would silently
//! stay in the graveyard instead of moving to hand.
//!
//! Three mutation-tested cases, all driven through the real activation +
//! resolution pipeline (`GameRunner::activate` + `GameAction::SelectCards`
//! answering the resulting `WaitingFor::SurveilChoice`):
//!   1. `chandra_moves_a_qualifying_card_from_graveyard_to_hand` — surveil put
//!      a noncreature, nonland card into the graveyard -> it ends up in HAND.
//!   2. `chandra_leaves_a_nonqualifying_creature_card_in_the_graveyard` —
//!      surveil put a CREATURE card into the graveyard -> it stays in the
//!      GRAVEYARD (negative control: the type filter is enforced, not "always
//!      move to hand").
//!   3. `chandra_keeping_the_card_on_top_never_moves_it_to_hand` — the
//!      controller chooses to keep the (qualifying) card on top of the
//!      library instead of putting it into the graveyard -> no move happens
//!      at all (negative control: the "this way" gate requires the graveyard
//!      branch specifically, not "surveil happened").
//!
//! CR ANCHORS:
//!   * CR 701.25a — to surveil N: look at the top N, put any number into the
//!     graveyard, the rest on top in any order.
//!   * CR 608.2c — the controller follows a resolving ability's instructions
//!     in printed order; later text may refer back to earlier text (the
//!     "that card" anaphor governing rule).
//!   * CR 614.6 — a destination-bound "this way" gate reads the card's actual
//!     arrival zone; `parse_you_put_into_graveyard_this_way_clause` pairs its
//!     filter with `destination: Some(Zone::Graveyard)` for exactly this
//!     reason (a redirect away from the graveyard would defeat it, mirroring
//!     the put-onto-battlefield sibling).

use engine::game::scenario::{GameRunner, GameScenario, P0};
use engine::types::ability::Effect;
use engine::types::actions::GameAction;
use engine::types::card_type::CoreType;
use engine::types::game_state::WaitingFor;
use engine::types::identifiers::ObjectId;
use engine::types::phase::Phase;
use engine::types::zones::Zone;

const CHANDRA_ORACLE: &str = concat!(
    "+1: Surveil 1. If you put a noncreature, nonland card into your ",
    "graveyard this way, put that card into your hand.\n",
    "+1: Add {U}. Spend this mana only to cast a noncreature spell.\n",
    "\u{2212}X: Tap target artifact or creature. Put X stun counters on it.\n",
    "\u{2212}6: You get an emblem with \"Whenever you cast a spell, draw a card.\"",
);

/// P0 controls Chandra plus a one-card library top, stamped with
/// `top_card_type` so the positive and negative-control cases can pin the
/// class filter ("noncreature, nonland") against a concrete core type.
fn setup(top_card_type: CoreType) -> (GameRunner, ObjectId, ObjectId) {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let chandra = scenario
        .add_planeswalker_from_oracle(
            P0,
            "Chandra, Chill of Compliance",
            "Chandra",
            3,
            CHANDRA_ORACLE,
        )
        .id();
    let top_card = scenario.add_card_to_library_top(P0, "Surveilled Card");

    let mut runner = scenario.build();
    {
        let obj = runner
            .state_mut()
            .objects
            .get_mut(&top_card)
            .expect("library card exists");
        obj.card_types.core_types = vec![top_card_type];
    }
    (runner, chandra, top_card)
}

/// Activate Chandra's first `[+1]` (the Surveil ability — located by effect
/// shape rather than a hardcoded index, mirroring `kaito_integration.rs`'s
/// `kaito_surveil_and_draw`) and answer the resulting `WaitingFor::
/// SurveilChoice` with `keep` on top.
///
/// Deliberately drives raw `GameAction`s rather than the `AbilityActivation`
/// builder's `.resolve()`: that shared driver's default `SurveilChoice`
/// policy keeps EVERY looked-at card on top (CR 701.25a's scry-like default,
/// mirroring `WaitingFor::ScryChoice`'s own default) — exactly the OPPOSITE
/// of what these tests need to exercise (some cards must reach the
/// graveyard). A generic `for _ in 0..N { match waiting_for { .. } }` loop
/// answers `Priority` with a pass and `SurveilChoice` with the caller's own
/// `keep` set instead of that library default.
fn activate_plus_one_and_surveil(runner: &mut GameRunner, chandra: ObjectId, keep: &[ObjectId]) {
    let surveil_ability_index = {
        let chandra_obj = runner.state().objects.get(&chandra).unwrap();
        chandra_obj
            .abilities
            .iter()
            .position(|ability| matches!(*ability.effect, Effect::Surveil { .. }))
            .expect("Chandra must have a Surveil loyalty ability")
    };
    runner
        .act(GameAction::ActivateAbility {
            source_id: chandra,
            ability_index: surveil_ability_index,
        })
        .expect("activating Chandra's [+1] Surveil ability must succeed");

    let mut answered_surveil_choice = false;
    for _ in 0..30 {
        match &runner.state().waiting_for {
            WaitingFor::SurveilChoice { .. } if !answered_surveil_choice => {
                answered_surveil_choice = true;
                runner
                    .act(GameAction::SelectCards {
                        cards: keep.to_vec(),
                    })
                    .expect("submit the surveil keep-on-top selection");
            }
            WaitingFor::Priority { .. } => {
                if runner.act(GameAction::PassPriority).is_err() {
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(
        answered_surveil_choice,
        "Chandra's [+1] must reach WaitingFor::SurveilChoice during resolution"
    );
}

/// Case 1 (positive): the surveilled card is an Instant — noncreature,
/// nonland — and the controller puts it into the graveyard (submits an empty
/// keep-on-top set). The reflexive gate must fire and move it to hand.
#[test]
fn chandra_moves_a_qualifying_card_from_graveyard_to_hand() {
    let (mut runner, chandra, card) = setup(CoreType::Instant);

    activate_plus_one_and_surveil(&mut runner, chandra, &[]);

    assert_eq!(
        runner.state().objects[&card].zone,
        Zone::Hand,
        "a noncreature, nonland card put into the graveyard this way must move to hand"
    );
    assert!(
        !runner.state().players[P0.0 as usize]
            .graveyard
            .contains(&card),
        "the card must not be left behind in the graveyard once moved to hand"
    );
}

/// Case 2 (negative control — type filter): the surveilled card is a
/// Creature. Putting it into the graveyard must NOT trigger the hand-move —
/// proving the "noncreature, nonland" filter is actually enforced, not a
/// vacuous always-move gate.
#[test]
fn chandra_leaves_a_nonqualifying_creature_card_in_the_graveyard() {
    let (mut runner, chandra, card) = setup(CoreType::Creature);

    activate_plus_one_and_surveil(&mut runner, chandra, &[]);

    assert_eq!(
        runner.state().objects[&card].zone,
        Zone::Graveyard,
        "a creature card put into the graveyard this way must NOT move to hand"
    );
}

/// Case 3 (negative control — gate requires the graveyard branch): the
/// surveilled card qualifies (Instant), but the controller keeps it on top of
/// the library instead of putting it into the graveyard. No move may happen
/// at all — proving the "this way" gate is bound to the graveyard branch of
/// the surveil choice, not to "a surveil happened".
#[test]
fn chandra_keeping_the_card_on_top_never_moves_it_to_hand() {
    let (mut runner, chandra, card) = setup(CoreType::Instant);

    activate_plus_one_and_surveil(&mut runner, chandra, &[card]);

    assert_eq!(
        runner.state().objects[&card].zone,
        Zone::Library,
        "a card kept on top by the surveil choice must stay in the library"
    );
    let library: Vec<ObjectId> = runner.state().players[P0.0 as usize]
        .library
        .iter()
        .copied()
        .collect();
    assert_eq!(
        library.first(),
        Some(&card),
        "the kept card must remain on top of the library"
    );
}
