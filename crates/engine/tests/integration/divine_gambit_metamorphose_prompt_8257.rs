//! Issue #8257 — resolution-time private-zone choices controlled by the
//! controller of an earlier target must not inherit that target object.
//!
//! Divine Gambit exiles the target, while Metamorphose moves it to the hidden
//! Library. In both cases the opponent, not the caster, owns the optional hand
//! choice. These tests drive the real cast/resolve pipeline with two eligible
//! cards so accepting must surface an `EffectZoneChoice`; declining must leave
//! both cards in hand.

use engine::game::scenario::{GameRunner, GameScenario, P0, P1};
use engine::types::phase::Phase;
use engine::types::zones::Zone;
use engine::types::ObjectId;

const DIVINE_GAMBIT: &str = "Exile target artifact, creature, or enchantment an opponent controls. That player may put a permanent card from their hand onto the battlefield.";
const METAMORPHOSE: &str = "Put target permanent an opponent controls on top of its owner's library. That opponent may put an artifact, creature, enchantment, or land card from their hand onto the battlefield.";

fn setup(oracle: &str, name: &str) -> (GameRunner, ObjectId, ObjectId, ObjectId, ObjectId) {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let spell = scenario
        .add_spell_to_hand_from_oracle(P0, name, false, oracle)
        .id();
    let target = scenario
        .add_artifact_from_oracle(P1, "Opponent Target", "")
        .id();
    let creature = scenario
        .add_creature_to_hand(P1, "Opponent Bear", 2, 2)
        .id();
    let land = scenario.add_land_to_hand(P1, "Opponent Forest").id();
    (scenario.build(), spell, target, creature, land)
}

#[test]
fn divine_gambit_accept_prompts_opponent_and_puts_chosen_permanent() {
    let (mut runner, spell, target, creature, land) = setup(DIVINE_GAMBIT, "Divine Gambit");

    let outcome = runner
        .cast(spell)
        .target_objects(&[target])
        .accept_optional()
        .effect_zone(&[creature])
        .resolve();

    assert_eq!(outcome.zone_of(target), Zone::Exile);
    assert_eq!(outcome.zone_of(creature), Zone::Battlefield);
    assert_eq!(outcome.zone_of(land), Zone::Hand);
}

#[test]
fn divine_gambit_decline_leaves_opponents_hand_untouched() {
    let (mut runner, spell, target, creature, land) = setup(DIVINE_GAMBIT, "Divine Gambit");

    let outcome = runner
        .cast(spell)
        .target_objects(&[target])
        .decline_optional()
        .resolve();

    assert_eq!(outcome.zone_of(target), Zone::Exile);
    assert_eq!(outcome.zone_of(creature), Zone::Hand);
    assert_eq!(outcome.zone_of(land), Zone::Hand);
}

#[test]
fn metamorphose_accept_prompts_opponent_after_hidden_library_move() {
    let (mut runner, spell, target, creature, land) = setup(METAMORPHOSE, "Metamorphose");

    let outcome = runner
        .cast(spell)
        .target_objects(&[target])
        .accept_optional()
        .effect_zone(&[land])
        .resolve();

    assert_eq!(outcome.zone_of(target), Zone::Library);
    assert_eq!(outcome.zone_of(land), Zone::Battlefield);
    assert_eq!(outcome.zone_of(creature), Zone::Hand);
}

#[test]
fn metamorphose_decline_leaves_opponents_hand_untouched() {
    let (mut runner, spell, target, creature, land) = setup(METAMORPHOSE, "Metamorphose");

    let outcome = runner
        .cast(spell)
        .target_objects(&[target])
        .decline_optional()
        .resolve();

    assert_eq!(outcome.zone_of(target), Zone::Library);
    assert_eq!(outcome.zone_of(creature), Zone::Hand);
    assert_eq!(outcome.zone_of(land), Zone::Hand);
}
