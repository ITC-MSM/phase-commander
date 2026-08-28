//! Runtime regression tests for Azog, Moria's Ruin:
//!
//! "When Azog enters, destroy up to one other target creature. Its controller
//! amasses Goblins X, where X is that creature's power. If you controlled
//! that creature, draw a card. (To amass Goblins X, that player puts X
//! +1/+1 counters on an Army they control. It's also a Goblin. If they don't
//! control an Army, they create a 0/0 black Goblin Army creature token
//! first.)"
//!
//! CR 701.47a: Amass — the AMASS PERFORMER here is the destroyed creature's
//! controller ("its controller"), not necessarily Azog's own controller —
//! distinct from every other printed "Amass [subtype] N" card, where the
//! ability's own controller always performs the amass. CR 109.4 anaphors
//! "its controller" to the parent target; CR 608.2h requires X
//! ("that creature's power") to read the destroyed creature's
//! last-known-information, since the creature has already left the
//! battlefield by the time Amass resolves in the same instruction.
//!
//! Three discriminating cases:
//! (a) Azog's controller (P0) destroys an OPPONENT's (P1) creature (power 3)
//!     — P1 (not P0) amasses Goblins 3, and P0 does NOT draw a card (CR
//!     109.4: "you" in "if you controlled that creature" is Azog's
//!     controller, who did not control the destroyed creature).
//! (b) Azog's controller (P0) destroys their OWN creature (power 2) — P0
//!     amasses Goblins 2 AND draws a card.
//! (c) Azog's controller declines the "up to one" target — no destroy, no
//!     amass, no draw, no crash (CR 601.2c optional targeting / CR 608.2c).

use engine::game::scenario::{GameScenario, P0, P1};
use engine::types::ability::TargetRef;
use engine::types::actions::GameAction;
use engine::types::counter::CounterType;
use engine::types::game_state::{CastPaymentMode, WaitingFor};
use engine::types::identifiers::ObjectId;
use engine::types::mana::{ManaType, ManaUnit};
use engine::types::phase::Phase;
use engine::types::player::PlayerId;
use engine::types::zones::Zone;

const AZOG: &str = "When Azog enters, destroy up to one other target creature. Its controller amasses Goblins X, where X is that creature's power. If you controlled that creature, draw a card. (To amass Goblins X, that player puts X +1/+1 counters on an Army they control. It's also a Goblin. If they don't control an Army, they create a 0/0 black Goblin Army creature token first.)";

fn black_mana(n: usize) -> Vec<ManaUnit> {
    (0..n)
        .map(|_| ManaUnit::new(ManaType::Black, ObjectId(0), false, vec![]))
        .collect()
}

fn hand_count(runner: &engine::game::scenario::GameRunner, player: PlayerId) -> usize {
    runner
        .state()
        .players
        .iter()
        .find(|p| p.id == player)
        .map(|p| p.hand.len())
        .unwrap_or(0)
}

/// Find the (single) Army creature controlled by `controller`, if any.
fn find_army(
    runner: &engine::game::scenario::GameRunner,
    controller: PlayerId,
) -> Option<&engine::game::game_object::GameObject> {
    let state = runner.state();
    state
        .battlefield
        .iter()
        .filter_map(|id| state.objects.get(id))
        .find(|obj| {
            obj.controller == controller && obj.card_types.subtypes.iter().any(|s| s == "Army")
        })
}

fn cast_azog(runner: &mut engine::game::scenario::GameRunner, azog: ObjectId) {
    let card_id = runner.state().objects[&azog].card_id;
    runner
        .act(GameAction::CastSpell {
            object_id: azog,
            card_id,
            targets: vec![],
            payment_mode: CastPaymentMode::Auto,
        })
        .expect("cast Azog, Moria's Ruin");
    runner.advance_until_stack_empty();

    assert!(
        matches!(
            runner.state().waiting_for,
            WaitingFor::TriggerTargetSelection { .. }
        ),
        "Azog's ETB must pause on trigger target selection, got {:?}",
        runner.state().waiting_for
    );
}

#[test]
fn azog_destroys_opponent_creature_opponent_amasses_and_azogs_controller_does_not_draw() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    scenario.with_mana_pool(P0, black_mana(3));

    // The opponent's 3-power creature is the destroy target. "Its controller
    // amasses Goblins X, where X is that creature's power" -> P1 amasses
    // Goblins 3. "If you controlled that creature, draw a card" -> P0 did
    // NOT control it, so P0 does not draw.
    let victim = scenario.add_creature(P1, "Grizzly Bears", 3, 3).id();
    let azog = scenario
        .add_creature_to_hand_from_oracle(P0, "Azog, Moria's Ruin", 1, 3, AZOG)
        .id();

    let mut runner = scenario.build();

    cast_azog(&mut runner, azog);
    // Baseline taken AFTER the cast (Azog has already left hand) so the
    // assertion below isolates the ETB rider's draw-or-not, not the cast's
    // own hand delta.
    let p0_hand_before_choice = hand_count(&runner, P0);

    runner
        .act(GameAction::ChooseTarget {
            target: Some(TargetRef::Object(victim)),
        })
        .expect("choose the destroy target");
    runner.advance_until_stack_empty();

    // The chosen creature was destroyed (CR 701.6b).
    assert_eq!(
        runner.state().objects[&victim].zone,
        Zone::Graveyard,
        "the targeted creature should have been destroyed by Azog's ETB"
    );

    // P1 (the destroyed creature's controller), not P0 (Azog's controller),
    // amassed Goblins 3.
    let army = find_army(&runner, P1)
        .expect("P1 (the destroyed creature's controller) should have amassed an Army");
    assert!(
        army.card_types.subtypes.iter().any(|s| s == "Goblin"),
        "the amassed Army must also be a Goblin"
    );
    assert_eq!(
        army.counters.get(&CounterType::Plus1Plus1).copied(),
        Some(3),
        "X must bind to the destroyed creature's power (3), read via LKI"
    );
    assert_eq!(army.power, Some(3));
    assert!(
        find_army(&runner, P0).is_none(),
        "Azog's controller (P0) must NOT amass — the destroyed creature's controller was P1"
    );

    // P0 did not control the destroyed creature, so P0 does not draw.
    assert_eq!(
        hand_count(&runner, P0),
        p0_hand_before_choice,
        "Azog's controller must not draw when they did not control the destroyed creature"
    );
}

#[test]
fn azog_destroys_own_creature_same_controller_amasses_and_draws() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    scenario.with_mana_pool(P0, black_mana(3));
    // The draw needs a real card to pull off the top of the library.
    scenario.with_library_top(P0, &["Forest"]);

    // Azog's own controller's 2-power creature is the destroy target. "Its
    // controller amasses Goblins X" -> P0 amasses Goblins 2. "If you
    // controlled that creature, draw a card" -> P0 DID control it, so P0
    // draws.
    let own_creature = scenario.add_creature(P0, "Runeclaw Bear", 2, 2).id();
    let azog = scenario
        .add_creature_to_hand_from_oracle(P0, "Azog, Moria's Ruin", 1, 3, AZOG)
        .id();

    let mut runner = scenario.build();

    cast_azog(&mut runner, azog);
    let p0_hand_before_choice = hand_count(&runner, P0);

    runner
        .act(GameAction::ChooseTarget {
            target: Some(TargetRef::Object(own_creature)),
        })
        .expect("choose the destroy target");
    runner.advance_until_stack_empty();

    assert_eq!(
        runner.state().objects[&own_creature].zone,
        Zone::Graveyard,
        "the targeted creature should have been destroyed by Azog's ETB"
    );

    // P0 amassed Goblins 2 (its own creature's power).
    let army =
        find_army(&runner, P0).expect("P0 should have amassed an Army from its own creature");
    assert!(army.card_types.subtypes.iter().any(|s| s == "Goblin"));
    assert_eq!(
        army.counters.get(&CounterType::Plus1Plus1).copied(),
        Some(2),
        "X must bind to the destroyed creature's power (2), read via LKI"
    );
    assert_eq!(army.power, Some(2));

    // P0 controlled the destroyed creature, so P0 draws a card.
    assert_eq!(
        hand_count(&runner, P0),
        p0_hand_before_choice + 1,
        "Azog's controller must draw when they controlled the destroyed creature"
    );
}

#[test]
fn azog_declining_the_up_to_one_target_amasses_and_draws_nothing() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);
    scenario.with_mana_pool(P0, black_mana(3));

    // A creature is on board for both players so a would-be amass/draw is
    // observable as absent, not merely untried.
    scenario.add_creature(P1, "Grizzly Bears", 3, 3);
    let own_creature = scenario.add_creature(P0, "Runeclaw Bear", 2, 2).id();
    let azog = scenario
        .add_creature_to_hand_from_oracle(P0, "Azog, Moria's Ruin", 1, 3, AZOG)
        .id();

    let mut runner = scenario.build();

    cast_azog(&mut runner, azog);
    // Baseline taken AFTER the cast (Azog has already left hand) so the
    // assertion below isolates the ETB rider's draw-or-not, not the cast's
    // own hand delta.
    let p0_hand_before_choice = hand_count(&runner, P0);

    runner
        .act(GameAction::ChooseTarget { target: None })
        .expect("decline the up-to-one destroy target");
    runner.advance_until_stack_empty();

    // Nothing was destroyed.
    assert_eq!(
        runner.state().objects[&own_creature].zone,
        Zone::Battlefield,
        "declining the target must not destroy anything"
    );

    // Nobody amassed.
    assert!(
        find_army(&runner, P0).is_none(),
        "declining the target must not amass for Azog's controller"
    );
    assert!(
        find_army(&runner, P1).is_none(),
        "declining the target must not amass for the opponent"
    );

    // Nobody drew (and, critically, resolution did not crash on the
    // no-target case — CR 608.2h with no LKI to read).
    assert_eq!(
        hand_count(&runner, P0),
        p0_hand_before_choice,
        "declining the target must not draw a card"
    );
    assert!(matches!(
        runner.state().waiting_for,
        WaitingFor::Priority { .. }
    ));
}
