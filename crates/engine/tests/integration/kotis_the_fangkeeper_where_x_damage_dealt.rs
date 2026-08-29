//! Regression for issue #5923: Kotis, the Fangkeeper's combat-damage trigger
//! must exile the top X cards of the damaged player's library (X = damage
//! dealt) and grant Kotis's controller permission to cast, from among just
//! that exiled batch, only the cards with mana value X or less.
//!
//! Before the `oracle_nom/quantity.rs` fix, the "where X is the amount of
//! damage dealt" binding was left unresolved (the bare-phrase arm only
//! matched "the damage dealt", not the "amount of" paraphrase), so the
//! totality guard in `oracle_effect/lower.rs` collapsed both the `ExileTop`
//! step and the `CastFromZone` sub-ability to `Effect::Unimplemented` and
//! neither the exile nor the free-cast offer ever happened.
//!
//! https://github.com/phase-rs/phase/issues/5923

use engine::game::casting::spell_objects_available_to_cast;
use engine::game::combat::AttackTarget;
use engine::game::scenario::{GameScenario, P0, P1};
use engine::types::actions::GameAction;
use engine::types::card_type::CoreType;
use engine::types::game_state::WaitingFor;
use engine::types::identifiers::ObjectId;
use engine::types::mana::ManaCost;
use engine::types::phase::Phase;
use engine::types::zones::Zone;

const KOTIS_ORACLE: &str = "Indestructible\nWhenever Kotis deals combat damage to a player, exile the top X cards of their library, where X is the amount of damage dealt. You may cast any number of spells with mana value X or less from among them without paying their mana costs.";

/// Drive combat until Kotis's trigger has fully resolved: declare no
/// blockers, order the single trigger, then pass priority until the stack is
/// empty and the expected top-of-library cards have left the library.
fn drain_until_kotis_trigger_resolves(
    runner: &mut engine::game::scenario::GameRunner,
    cheap: ObjectId,
    expensive: ObjectId,
) {
    for _ in 0..64 {
        match runner.state().waiting_for.clone() {
            WaitingFor::OrderTriggers { .. } => {
                runner
                    .act(GameAction::OrderTriggers { order: vec![0] })
                    .expect("order Kotis's trigger");
            }
            WaitingFor::DeclareBlockers { .. } => {
                runner
                    .act(GameAction::DeclareBlockers {
                        assignments: vec![],
                    })
                    .expect("declare no blockers");
            }
            // CR 603.5 + CR 608.2d: the "you may cast ... from among them"
            // sub-ability is a "may" effect — accept it so the CastFromZone
            // permission is actually granted onto the exiled batch.
            WaitingFor::OptionalEffectChoice { .. } => {
                runner
                    .act(GameAction::DecideOptionalEffect { accept: true })
                    .expect("accept the optional cast-from-exile grant");
            }
            WaitingFor::Priority { .. } => {
                if runner.state().stack.is_empty()
                    && runner.state().objects[&cheap].zone == Zone::Exile
                    && runner.state().objects[&expensive].zone == Zone::Exile
                {
                    return;
                }
                runner
                    .act(GameAction::PassPriority)
                    .expect("pass priority while draining Kotis's trigger");
            }
            other => panic!(
                "unexpected waiting state while draining Kotis's trigger: {other:?} \
                 (phase={:?})",
                runner.state().phase
            ),
        }
    }
    panic!("Kotis's trigger did not resolve");
}

/// CR 120.2a + CR 608.2h: Kotis deals 3 combat damage (each attacking
/// creature deals combat damage equal to its power), so X = 3. The top three
/// library cards are exiled (two within budget, MV 1 and MV 3; one over
/// budget, MV 5) and a fourth card stays in the library, proving the exile is
/// bounded to exactly X cards from the DAMAGED player's library, not Kotis's
/// controller's.
///
/// The "any number of spells ... from among them" grant is a BATCH
/// permission, not a single pick: both eligible cards in the SAME batch must
/// be independently castable (not just the first one found), the over-budget
/// card in that same batch must stay unavailable, and — critically — a
/// separately exiled, unrelated, otherwise-eligible card that Kotis never
/// touched must ALSO stay unavailable. Without the `ExiledBySource` link
/// scoping the permission to just this trigger's own exiled batch, "from
/// among them" would incorrectly widen to "any eligible card anywhere in
/// exile" and let the controller cast cards Kotis never exiled.
#[test]
fn kotis_exiles_top_x_cards_and_offers_only_mana_value_x_or_less() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    // P0 (Kotis's controller) has its own library card that must NOT be
    // touched by Kotis's trigger.
    let controller_top = scenario.add_card_to_library_top(P0, "Controller Top");

    // P1 (the damaged player) library, top-to-bottom after seeding:
    // Cheap One (MV 1, within budget) -> Cheap Two (MV 3, within budget) ->
    // Expensive Card (MV 5, over budget) -> Filler Card (must remain in
    // library; outside the top-X window).
    let filler = scenario.add_card_to_library_top(P1, "Filler Card");
    let expensive = scenario.add_card_to_library_top(P1, "Expensive Card");
    let cheap_two = scenario.add_card_to_library_top(P1, "Cheap Two");
    let cheap_one = scenario.add_card_to_library_top(P1, "Cheap One");

    // A card exiled through an entirely unrelated mechanism (never touched by
    // Kotis), within Kotis's mana-value budget and otherwise perfectly
    // eligible — the negative control for "from among them" batch scoping.
    let unrelated_exiled = scenario
        .add_spell_to_exile(P1, "Unrelated Exiled Card", true)
        .with_mana_cost(ManaCost::generic(1))
        .id();

    let kotis = scenario
        .add_creature(P0, "Kotis, the Fangkeeper", 3, 1)
        .from_oracle_text_with_keywords(&["Indestructible"], KOTIS_ORACLE)
        .id();

    let mut runner = scenario.build();
    {
        let card = runner.state_mut().objects.get_mut(&cheap_one).unwrap();
        card.card_types.core_types.push(CoreType::Instant);
        card.mana_cost = ManaCost::Cost {
            shards: Vec::new(),
            generic: 1,
        };
    }
    {
        let card = runner.state_mut().objects.get_mut(&cheap_two).unwrap();
        card.card_types.core_types.push(CoreType::Instant);
        card.mana_cost = ManaCost::Cost {
            shards: Vec::new(),
            generic: 3,
        };
    }
    {
        let card = runner.state_mut().objects.get_mut(&expensive).unwrap();
        card.card_types.core_types.push(CoreType::Instant);
        card.mana_cost = ManaCost::Cost {
            shards: Vec::new(),
            generic: 5,
        };
    }

    runner.pass_both_players();
    runner
        .act(GameAction::DeclareAttackers {
            attacks: vec![(kotis, AttackTarget::Player(P1))],
            bands: vec![],
        })
        .expect("declare Kotis attacking P1");

    drain_until_kotis_trigger_resolves(&mut runner, cheap_one, expensive);

    let state = runner.state();

    // Exactly the top three P1 library cards were exiled; the fourth stays
    // put, and P0's own library is untouched. "Their library" is an
    // Oracle-text grammar interpretation — the pronoun binds to the nearest
    // preceding player noun, the damaged player from "deals combat damage to
    // a player," not Kotis's controller — not a claim covered by a specific
    // CR number (CR 608.2c governs the ORDER effects apply their
    // instructions, not pronoun antecedents).
    assert_eq!(state.objects[&cheap_one].zone, Zone::Exile);
    assert_eq!(state.objects[&cheap_two].zone, Zone::Exile);
    assert_eq!(state.objects[&expensive].zone, Zone::Exile);
    assert_eq!(
        state.objects[&filler].zone,
        Zone::Library,
        "only the top X (3) cards may be exiled, not the whole library"
    );
    assert_eq!(
        state.objects[&controller_top].zone,
        Zone::Library,
        "Kotis must exile from the DAMAGED player's library, not its controller's"
    );

    // The free-cast offer is scoped to the exiled batch AND bounded by X (3):
    // BOTH within-budget cards in the batch are offered, the over-budget card
    // in the SAME batch is not, and a separately exiled, unrelated,
    // within-budget card Kotis never touched is not offered either.
    let available = spell_objects_available_to_cast(state, P0);
    assert!(
        available.contains(&cheap_one),
        "a mana value 1 card (<= X=3) exiled by Kotis must be offered for free casting"
    );
    assert!(
        available.contains(&cheap_two),
        "a SECOND mana value 3 card (<= X=3) in the SAME batch must also be offered — \
         \"any number\" is a batch grant, not a single pick"
    );
    assert!(
        !available.contains(&expensive),
        "a mana value 5 card (> X=3) must NOT be offered even though it was exiled in the same batch"
    );
    assert!(
        !available.contains(&unrelated_exiled),
        "a mana value 1 card exiled by an UNRELATED mechanism must NOT be offered — \
         \"from among them\" must not widen to every eligible card sitting in exile"
    );

    // Behavioral confirmation: BOTH within-budget cards can actually be cast
    // for free through the granted permission, proving the batch grant is not
    // exhausted after the first cast.
    let cast_outcome = runner.cast(cheap_one).resolve();
    cast_outcome.assert_zone(&[cheap_one], Zone::Graveyard);
    let cast_outcome = runner.cast(cheap_two).resolve();
    cast_outcome.assert_zone(&[cheap_two], Zone::Graveyard);
}
