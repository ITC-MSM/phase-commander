//! GitHub issue #8160 — Carmen, Cruel Skymarcher gained +1/+1 counters for
//! sacrifices that happened while she was off the battlefield.
//!
//! Oracle (verified against Scryfall and `client/public/card-data.json`):
//!   "Flying
//!    Whenever a player sacrifices a permanent, put a +1/+1 counter on Carmen
//!    and you gain 1 life.
//!    Whenever Carmen attacks, return up to one target permanent card with
//!    mana value less than or equal to Carmen's power from your graveyard to
//!    the battlefield."
//!
//! CR 113.6: "Abilities of all other objects [than instants/sorceries]
//! usually function only while that object is on the battlefield." CR 603.2:
//! a triggered ability only triggers when a matching event occurs while the
//! ability is live. Carmen's "whenever a player sacrifices a permanent"
//! ability is not live while she is in the graveyard, so a sacrifice that
//! happens during that window must never grant her a counter — including a
//! sacrifice that happens EARLIER IN THE SAME RESOLUTION as the effect that
//! puts her back onto the battlefield (Living Death: "Each player exiles all
//! creature cards from their graveyard, then sacrifices all creatures they
//! control, then puts all cards they exiled this way onto the battlefield.").
//!
//! Root cause: `collect_pending_triggers_with_collection`
//! (`crates/engine/src/game/triggers.rs`) receives the WHOLE accumulated
//! `events` list for one resolved ability chain (CR 608.2c: a chain's steps
//! execute in the order written, publishing into one shared events buffer)
//! and scans it once, at the end, against END-of-chain live object state.
//! The candidate gate `live_battlefield_source_was_present_at_event` used to
//! exclude only the trivial case of an object's OWN departure event, so a
//! permanent that (re-)entered the battlefield partway through the batch
//! incorrectly matched an EARLIER event in the same batch, before it was
//! there to observe it. The fix tracks each object's last battlefield-entry
//! index within the batch and refuses to match any event before it.

use engine::game::scenario::{GameScenario, P0};
use engine::types::counter::CounterType;
use engine::types::identifiers::ObjectId;
use engine::types::mana::{ManaType, ManaUnit};
use engine::types::phase::Phase;
use engine::types::zones::Zone;

fn mana(color: ManaType) -> ManaUnit {
    ManaUnit::new(color, ObjectId(0), false, vec![])
}

const CARMEN_ORACLE: &str = "Flying\nWhenever a player sacrifices a permanent, put a +1/+1 counter on Carmen and you gain 1 life.\nWhenever Carmen attacks, return up to one target permanent card with mana value less than or equal to Carmen's power from your graveyard to the battlefield.";

const LIVING_DEATH_ORACLE: &str = "Each player exiles all creature cards from their graveyard, then sacrifices all creatures they control, then puts all cards they exiled this way onto the battlefield.";

const VILLAGE_RITES_ORACLE: &str =
    "As an additional cost to cast this spell, sacrifice a creature.\nDraw two cards.";

/// Negative control (the reported bug): Carmen starts in the graveyard.
/// Living Death's own "sacrifice all creatures you control" step (CR 608.2c
/// step 2) sacrifices an unrelated battlefield creature BEFORE step 3 puts
/// Carmen back onto the battlefield. Carmen must end with zero +1/+1
/// counters — she was not on the battlefield when the sacrifice happened,
/// even though it occurred earlier in the very same resolution.
#[test]
fn carmen_does_not_gain_counter_for_sacrifice_before_her_own_return() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    // Carmen is already in the graveyard (e.g., she died earlier in the game).
    let carmen = scenario
        .add_creature_to_graveyard(P0, "Carmen, Cruel Skymarcher", 2, 2)
        .from_oracle_text(CARMEN_ORACLE)
        .id();

    // A second battlefield creature that Living Death's own sacrifice step
    // sweeps up before Carmen returns. This is the reach guard: if it is
    // still on the battlefield afterward, the sacrifice never happened and
    // the test proves nothing.
    let bears = scenario.add_creature(P0, "Grizzly Bears", 2, 2).id();

    let living_death = scenario
        .add_spell_to_hand_from_oracle(P0, "Living Death", false, LIVING_DEATH_ORACLE)
        .id();

    scenario.with_mana_pool(
        P0,
        vec![
            mana(ManaType::Colorless),
            mana(ManaType::Colorless),
            mana(ManaType::Colorless),
            mana(ManaType::Black),
            mana(ManaType::Black),
        ],
    );

    let mut runner = scenario.build();
    let outcome = runner.cast(living_death).resolve();

    // Reach guard: the sacrifice actually happened (Bears never returns —
    // only cards already in the graveyard before resolution are exiled and
    // returned).
    assert_eq!(
        outcome.state().objects[&bears].zone,
        Zone::Graveyard,
        "Living Death's own sacrifice step must have sent Grizzly Bears to the graveyard"
    );
    // Reach guard: Carmen actually came back to the battlefield, so this is
    // testing the "returned mid-chain" case and not a no-op.
    assert_eq!(
        outcome.state().objects[&carmen].zone,
        Zone::Battlefield,
        "Living Death must return Carmen (an exiled graveyard creature card) to the battlefield"
    );

    let counters = outcome.state().objects[&carmen]
        .counters
        .get(&CounterType::Plus1Plus1)
        .copied()
        .unwrap_or(0);
    assert_eq!(
        counters, 0,
        "CR 113.6 + CR 603.2: Carmen's sacrifice trigger was not live while she was in the \
         graveyard, so she must not retroactively gain a +1/+1 counter for a sacrifice that \
         happened earlier in the same Living Death resolution, before she returned"
    );
}

/// Positive control: Carmen is already on the battlefield when an unrelated
/// creature is sacrificed as an announced cost. Her trigger must still fire
/// normally — proves the fix does not disable the ordinary on-battlefield
/// case it must keep working.
#[test]
fn carmen_gains_counter_for_sacrifice_while_on_battlefield() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    let carmen = scenario
        .add_creature(P0, "Carmen, Cruel Skymarcher", 2, 2)
        .from_oracle_text(CARMEN_ORACLE)
        .id();
    let bears = scenario.add_creature(P0, "Grizzly Bears", 2, 2).id();

    let village_rites = scenario
        .add_spell_to_hand_from_oracle(P0, "Village Rites", true, VILLAGE_RITES_ORACLE)
        .id();
    scenario.with_mana_pool(P0, vec![mana(ManaType::Black)]);
    // Village Rites draws two cards — give P0 a library so the draw does not
    // empty it and cause a CR 104.3c game loss, which would otherwise exile
    // every object P0 owns (including Carmen and Bears) as elimination
    // cleanup and make the reach guards below fail for an unrelated reason.
    scenario.add_card_to_library_top(P0, "Plains");
    scenario.add_card_to_library_top(P0, "Plains");

    let mut runner = scenario.build();
    let outcome = runner
        .cast(village_rites)
        .sacrifice_with(&[bears])
        .resolve();

    // Reach guard: the sacrifice actually happened.
    assert_eq!(
        outcome.state().objects[&bears].zone,
        Zone::Graveyard,
        "Village Rites' additional cost must have sacrificed Grizzly Bears"
    );

    let counters = outcome.state().objects[&carmen]
        .counters
        .get(&CounterType::Plus1Plus1)
        .copied()
        .unwrap_or(0);
    assert_eq!(
        counters, 1,
        "Carmen was on the battlefield for the sacrifice, so her trigger must fire normally"
    );
    outcome.assert_life_delta(P0, 1);
}
