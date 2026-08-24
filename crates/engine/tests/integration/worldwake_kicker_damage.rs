//! Worldwake completion: Rumbling Aftershocks and Deathforge Shaman.
//!
//! This scenario parses both cards from their verbatim Oracle text, casts the
//! Shaman through the real cast/payment pipeline with multikicker paid twice,
//! and resolves both damage triggers. The resulting damage events distinguish
//! the two required authorities: 2 from the triggering spell's kick count and
//! 4 from twice the entering Shaman's own kick count.

use engine::game::scenario::{GameScenario, P0, P1};
use engine::types::ability::TargetRef;
use engine::types::actions::GameAction;
use engine::types::events::GameEvent;
use engine::types::game_state::{CastPaymentMode, WaitingFor};
use engine::types::mana::{ManaColor, ManaCost, ManaCostShard};
use engine::types::phase::Phase;

const RUMBLING_AFTERSHOCKS: &str = "Whenever you cast a kicked spell, you may have this enchantment deal damage to any target equal to the number of times that spell was kicked.";

const DEATHFORGE_SHAMAN: &str = "Multikicker {R} (You may pay an additional {R} any number of times as you cast this spell.)\nWhen this creature enters, it deals damage to target player or planeswalker equal to twice the number of times it was kicked.";

#[test]
fn worldwake_kicker_damage_cards_resolve_with_their_correct_counts() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    scenario.with_life(P1, 20);
    scenario.add_enchantment_from_oracle(P0, "Rumbling Aftershocks", RUMBLING_AFTERSHOCKS);
    let shaman = scenario
        .add_creature_to_hand_from_oracle(P0, "Deathforge Shaman", 4, 3, DEATHFORGE_SHAMAN)
        .with_mana_cost(ManaCost::Cost {
            generic: 4,
            shards: vec![ManaCostShard::Red],
        })
        .id();
    for _ in 0..8 {
        scenario.add_basic_land(P0, ManaColor::Red);
    }

    let mut runner = scenario.build();
    let card_id = runner.state().objects[&shaman].card_id;
    let mut events = runner
        .act(GameAction::CastSpell {
            object_id: shaman,
            card_id,
            targets: vec![],
            payment_mode: CastPaymentMode::Auto,
        })
        .expect("cast Deathforge Shaman")
        .events;

    let mut kicks_paid = 0;
    for _ in 0..80 {
        match runner.state().waiting_for.clone() {
            WaitingFor::OptionalCostChoice { .. } => {
                let pay = kicks_paid < 2;
                let result = runner
                    .act(GameAction::DecideOptionalCost { pay })
                    .expect("decide Deathforge Shaman multikicker");
                events.extend(result.events);
                if pay {
                    kicks_paid += 1;
                }
            }
            WaitingFor::TriggerTargetSelection { .. } | WaitingFor::TargetSelection { .. } => {
                let result = runner
                    .act(GameAction::ChooseTarget {
                        target: Some(TargetRef::Player(P1)),
                    })
                    .expect("target the opposing player");
                events.extend(result.events);
            }
            WaitingFor::OptionalEffectChoice { .. } => {
                let result = runner
                    .act(GameAction::DecideOptionalEffect { accept: true })
                    .expect("accept Rumbling Aftershocks' optional damage");
                events.extend(result.events);
            }
            WaitingFor::Priority { .. } => {
                let result = runner
                    .act(GameAction::PassPriority)
                    .expect("pass priority while resolving the cast and triggers");
                events.extend(result.events);
            }
            other => panic!("unexpected prompt while resolving Worldwake cards: {other:?}"),
        }
        if runner.life(P1) == 14 {
            break;
        }
    }

    assert_eq!(
        kicks_paid, 2,
        "the real cast pipeline must record two kicks"
    );
    assert_eq!(runner.life(P1), 14);

    let damage_amounts: Vec<u32> = events
        .iter()
        .filter_map(|event| match event {
            GameEvent::DamageDealt {
                target: TargetRef::Player(player),
                amount,
                ..
            } if *player == P1 => Some(*amount),
            _ => None,
        })
        .collect();
    assert!(
        damage_amounts.contains(&2),
        "Rumbling Aftershocks must deal 2 from the triggering spell's two kicks: {damage_amounts:?}"
    );
    assert!(
        damage_amounts.contains(&4),
        "Deathforge Shaman must deal twice its own two kicks: {damage_amounts:?}"
    );
}
