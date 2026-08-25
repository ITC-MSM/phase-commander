//! Reality Fracture coverage for Bloodline Recollector's creature-death threshold.

use engine::game::game_object::BackFaceData;
use engine::game::scenario::{GameRunner, GameScenario, P0, P1};
use engine::parser::oracle::parse_oracle_text;
use engine::types::ability::{
    Comparator, QuantityExpr, QuantityRef, TargetFilter, TriggerCondition, TypeFilter,
};
use engine::types::actions::GameAction;
use engine::types::card::LayoutKind;
use engine::types::game_state::WaitingFor;
use engine::types::phase::Phase;
use engine::types::zones::Zone;

const BLOODLINE_ORACLE: &str = "At the beginning of each end step, if three or more creatures died this turn, this creature becomes prepared. (While it's prepared, you may cast a copy of its spell. Doing so unprepares it.)";

fn setup(death_count: usize) -> (GameRunner, engine::types::identifiers::ObjectId) {
    let parsed = parse_oracle_text(
        BLOODLINE_ORACLE,
        "Bloodline Recollector",
        &[],
        &["Creature".to_string()],
        &[],
    );
    let trigger = parsed
        .triggers
        .first()
        .expect("Bloodline must parse its end-step trigger");
    assert!(
        !serde_json::to_string(trigger)
            .expect("serialize trigger")
            .contains("\"Unimplemented\""),
        "verbatim Bloodline Oracle must contain no Unimplemented node"
    );
    let Some(TriggerCondition::QuantityComparison {
        lhs:
            QuantityExpr::Ref {
                qty:
                    QuantityRef::ZoneChangeCountThisTurn {
                        from,
                        to,
                        filter: TargetFilter::Typed(filter),
                    },
            },
        comparator,
        rhs,
    }) = trigger.condition.as_ref()
    else {
        panic!("expected the typed creature-death threshold condition")
    };
    assert_eq!(*from, Some(Zone::Battlefield));
    assert_eq!(*to, Some(Zone::Graveyard));
    assert!(filter.type_filters.contains(&TypeFilter::Creature));
    assert_eq!(*comparator, Comparator::GE);
    assert_eq!(*rhs, QuantityExpr::Fixed { value: 3 });

    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    let source = scenario
        .add_creature(P0, "Bloodline Recollector", 2, 2)
        .from_oracle_text(BLOODLINE_ORACLE)
        .id();

    let mut victims = Vec::new();
    let mut removal = Vec::new();
    for index in 0..death_count {
        victims.push(
            scenario
                .add_creature(P1, &format!("Threshold Victim {index}"), 2, 2)
                .id(),
        );
        removal.push(
            scenario
                .add_spell_to_hand_from_oracle(
                    P0,
                    &format!("Threshold Removal {index}"),
                    true,
                    "Destroy target creature.",
                )
                .id(),
        );
    }

    let mut runner = scenario.build();
    runner
        .state_mut()
        .objects
        .get_mut(&source)
        .unwrap()
        .back_face = Some(BackFaceData {
        layout_kind: Some(LayoutKind::Prepare),
        ..BackFaceData::default()
    });

    for (spell, victim) in removal.into_iter().zip(victims) {
        runner.cast(spell).target_object(victim).resolve();
    }
    (runner, source)
}

fn source_trigger_count(
    runner: &GameRunner,
    source: engine::types::identifiers::ObjectId,
) -> usize {
    runner
        .state()
        .stack
        .iter()
        .filter(|entry| entry.source_id == source)
        .count()
}

fn advance_to_end_step_trigger(
    runner: &mut GameRunner,
    source: engine::types::identifiers::ObjectId,
) {
    for _ in 0..200 {
        if source_trigger_count(runner, source) > 0
            || (runner.state().phase == Phase::End
                && runner.state().stack.is_empty()
                && matches!(runner.state().waiting_for, WaitingFor::Priority { .. }))
        {
            return;
        }
        match runner.state().waiting_for.clone() {
            WaitingFor::Priority { .. } => runner
                .act(GameAction::PassPriority)
                .expect("pass priority while advancing to the end step"),
            WaitingFor::DeclareAttackers { .. } => runner
                .act(GameAction::DeclareAttackers {
                    attacks: vec![],
                    bands: vec![],
                })
                .expect("declare no attackers"),
            WaitingFor::DeclareBlockers { .. } => runner
                .act(GameAction::DeclareBlockers {
                    assignments: vec![],
                })
                .expect("declare no blockers"),
            other => panic!("unexpected waiting state before end step: {other:?}"),
        };
    }
    panic!("phase machine did not reach the end-step trigger");
}

#[test]
fn bloodline_threshold_gates_at_two_and_fires_at_three() {
    let (mut below, source) = setup(2);
    advance_to_end_step_trigger(&mut below, source);
    assert_eq!(source_trigger_count(&below, source), 0);
    assert!(below.state().objects[&source].prepared.is_none());

    let (mut exact, source) = setup(3);
    advance_to_end_step_trigger(&mut exact, source);
    assert_eq!(source_trigger_count(&exact, source), 1);
    exact.advance_until_stack_empty();
    assert!(exact.state().objects[&source].prepared.is_some());
}

#[test]
fn bloodline_intervening_if_is_rechecked_on_resolution() {
    let (mut runner, source) = setup(3);
    advance_to_end_step_trigger(&mut runner, source);
    assert_eq!(source_trigger_count(&runner, source), 1);

    // CR 603.4 requires a live resolution-time recheck. Death history is
    // monotonic during ordinary play, so clear the observed ledger after the
    // trigger reaches the stack to make a skipped recheck observably wrong.
    runner.state_mut().zone_changes_this_turn.clear();
    runner.advance_until_stack_empty();
    assert!(runner.state().objects[&source].prepared.is_none());
}
