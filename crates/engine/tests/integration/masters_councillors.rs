//! Master's Councillors — "Vigilance\nThis creature gets +2/+0 for each
//! graveyard with seven or more cards in it.\nWhenever you draw your second
//! card each turn, target player mills three cards. (They put the top three
//! cards of their library into their graveyard.)"
//!
//! This drives the REAL parse -> layers pipeline for the static P/T boost, and
//! the REAL parse -> trigger -> stack -> targeting -> resolution pipeline for
//! the second-draw mill trigger. Master's Councillors is built from Oracle
//! text via the scenario harness (production synthesis path), exactly like
//! the sibling `moon_girl_second_draw_base_pt` / `construct_cosmic_cube_second_draw_token`
//! second-draw fixtures.
//!
//! THE BUG `masters_councillors_pt_scales_with_graveyard_census_across_players`
//! discriminates: before the fix, "for each graveyard with seven or more cards
//! in it" failed to parse (`DynamicQty` swallow), so the static ability froze
//! at a flat +2/+0 regardless of graveyard sizes. Assertion (a) proves the
//! frozen-flat-boost misparse would already be wrong at zero qualifying
//! graveyards (base P/T would read 3/3, not 1/3); assertion (c) proves the
//! census counts EVERY qualifying graveyard in the game (CR 404.1), not just
//! the controller's own -- a controller-only misparse would plateau at 3/3
//! instead of climbing to 5/3.
//!
//! `masters_councillors_second_draw_mills_chosen_target_once_per_turn` proves
//! the `TriggerConstraint::NthDrawThisTurn { n: 2 }` gate (already a general
//! building block, shared with Moon Girl and Construct a Cosmic Cube) drives a
//! genuinely TARGETED trigger end to end: the controller's first draw does
//! nothing, the second requires and resolves a player target (a mill, not a
//! self-referential effect), and a third draw the same turn does not re-fire.

use engine::game::effects::draw::resolve as resolve_draw;
use engine::game::layers::evaluate_layers;
use engine::game::scenario::{GameRunner, GameScenario};
use engine::game::triggers::process_triggers;
use engine::game::zones::create_object;
use engine::types::ability::{Effect, QuantityExpr, ResolvedAbility, TargetFilter, TargetRef};
use engine::types::actions::GameAction;
use engine::types::game_state::WaitingFor;
use engine::types::identifiers::{CardId, ObjectId};
use engine::types::phase::Phase;
use engine::types::player::PlayerId;
use engine::types::zones::Zone;

const P0: PlayerId = PlayerId(0);
const P1: PlayerId = PlayerId(1);
const P2: PlayerId = PlayerId(2);
const P3: PlayerId = PlayerId(3);

const MASTERS_COUNCILLORS: &str = "Vigilance\nThis creature gets +2/+0 for each graveyard with seven or more cards in it.\nWhenever you draw your second card each turn, target player mills three cards. (They put the top three cards of their library into their graveyard.)";

fn effective_pt(runner: &mut GameRunner, id: ObjectId) -> (i32, i32) {
    runner.state_mut().layers_dirty.mark_full();
    evaluate_layers(runner.state_mut());
    let obj = &runner.state().objects[&id];
    (
        obj.power.expect("has power"),
        obj.toughness.expect("has toughness"),
    )
}

/// Count of cards in `player`'s graveyard (CR 404.1). `GameRunner` has no
/// built-in accessor for this (unlike `battlefield_count`/`life`), so this
/// reads the zone directly off state.
fn graveyard_len(runner: &GameRunner, player: PlayerId) -> usize {
    runner.state().players[player.0 as usize].graveyard.len()
}

/// Move `count` generic, ruleless cards into `player`'s graveyard through the
/// REAL zone-move pipeline (`zones::move_to_zone`), mirroring an ordinary
/// discard/mill rather than seeding the destination zone directly. This is
/// the seam the zone-invalidation regression requires: a card entering
/// `Zone::Graveyard` via `move_to_zone` -- not `create_object` straight into
/// the destination zone -- is what exercises
/// `any_active_static_reads_zone_membership` at the production zone-change
/// site (`zones.rs`), so a test that only ever seeds the graveyard directly
/// (as `GameScenario::with_graveyard` and the old version of this helper did)
/// can stay green even if that invalidation path is broken. Returns the moved
/// objects' ids (stable across the move -- CR 400.7's "becomes a new object"
/// is a game-rules fiction about characteristics, not an engine id reissue).
fn move_cards_into_graveyard(
    runner: &mut GameRunner,
    player: PlayerId,
    count: usize,
    label: &str,
) -> Vec<ObjectId> {
    let mut moved = Vec::with_capacity(count);
    for i in 0..count {
        let card_id = CardId(runner.state().next_object_id);
        let id = create_object(
            runner.state_mut(),
            card_id,
            player,
            format!("{label} {i}"),
            Zone::Hand,
        );
        let mut events = Vec::new();
        engine::game::zones::move_to_zone(runner.state_mut(), id, Zone::Graveyard, &mut events);
        moved.push(id);
    }
    moved
}

/// CR 404.1: the static P/T boost is a CENSUS over every player's graveyard,
/// not just the controller's -- (a) zero qualifying graveyards leaves the
/// creature at its printed 1/3, (b) the controller's OWN graveyard crossing
/// seven cards adds +2/+0, (c) a SECOND, unrelated player's graveyard also
/// crossing seven cards stacks a second +2/+0 (proving the count is global),
/// and (d) a graveyard re-crossing the threshold DOWNWARD drops its
/// contribution again.
///
/// Every threshold transition here happens through the REAL `move_to_zone`
/// zone-change pipeline (`move_cards_into_graveyard`), and each transition
/// asserts `layers_dirty.is_dirty()` BEFORE the forced recompute in
/// `effective_pt`. This is the zone-invalidation regression: before
/// `QuantityRef::PlayerCount { filter: PlayerAttribute { attr: GraveyardSize,
/// .. } }` was classified as reading the graveyard zone, an ORDINARY
/// mill/discard move (not Master's Councillors' own ability) never dirtied
/// the layer cache, so the +2/+0 census could go stale until an unrelated
/// full refresh happened to occur. Forcing `mark_full` unconditionally (as
/// the old version of this test did before every assertion) would mask
/// exactly that bug -- these `is_dirty()` checks are the seam that catches it.
#[test]
fn masters_councillors_pt_scales_with_graveyard_census_across_players() {
    let mut scenario = GameScenario::new_n_player(4, 42);
    scenario.at_phase(Phase::PreCombatMain);
    let mc = scenario
        .add_creature_from_oracle(P0, "Master's Councillors", 1, 3, MASTERS_COUNCILLORS)
        .id();
    // P0's own graveyard starts one card short of the threshold.
    scenario.with_graveyard(
        P0,
        &[
            "Filler A", "Filler B", "Filler C", "Filler D", "Filler E", "Filler F",
        ],
    );
    let mut runner = scenario.build();

    // (a) Zero qualifying graveyards (P0 has 6, P1/P2/P3 have 0): base 1/3.
    assert_eq!(graveyard_len(&runner, P0), 6);
    assert_eq!(
        effective_pt(&mut runner, mc),
        (1, 3),
        "no graveyard has reached seven cards yet -- Master's Councillors is a plain 1/3"
    );
    assert!(
        !runner.state().layers_dirty.is_dirty(),
        "precondition: the layer cache is clean before the first zone move"
    );

    // (b) P0's own graveyard crosses the threshold (7th card) via the REAL
    // zone-move pipeline: +2/+0 -> 3/3.
    move_cards_into_graveyard(&mut runner, P0, 1, "Filler G");
    assert_eq!(graveyard_len(&runner, P0), 7);
    assert!(
        runner.state().layers_dirty.is_dirty(),
        "THE FIX: crossing the seven-card graveyard threshold via a normal zone move \
         must dirty the layer cache, not just Master's Councillors' own mill"
    );
    assert_eq!(
        effective_pt(&mut runner, mc),
        (3, 3),
        "CR 404.1: one qualifying graveyard (the controller's own) adds +2/+0"
    );
    assert!(!runner.state().layers_dirty.is_dirty());

    // (c) P2 -- NOT the controller -- also reaches seven cards through the
    // same production pipeline: the boost stacks to +4/+0 total, proving the
    // census counts every player's graveyard in the game, not just the
    // controller's, AND that the invalidation fires again for a second,
    // independent player's zone move.
    move_cards_into_graveyard(&mut runner, P2, 7, "P2 Filler");
    assert_eq!(graveyard_len(&runner, P2), 7);
    assert!(
        runner.state().layers_dirty.is_dirty(),
        "a second, unrelated player's graveyard crossing the threshold must also dirty \
         the layer cache"
    );
    assert_eq!(
        effective_pt(&mut runner, mc),
        (5, 3),
        "CR 404.1: TWO qualifying graveyards (P0's and P2's) add +4/+0 total"
    );

    // Sanity: P1 and P3 never crossed the threshold and contribute nothing.
    assert_eq!(graveyard_len(&runner, P1), 0);
    assert_eq!(graveyard_len(&runner, P3), 0);

    // (d) RE-CROSS DOWNWARD: an unrelated effect exiles a card straight out of
    // P0's graveyard, dropping it back to six cards -- below the threshold.
    // The dependency walker must dirty the cache on the way back down too,
    // not only when a graveyard first reaches seven.
    assert!(!runner.state().layers_dirty.is_dirty());
    let p0_gy_card = runner.state().players[P0.0 as usize]
        .graveyard
        .last()
        .copied()
        .expect("P0's graveyard has a card to exile");
    let mut events = Vec::new();
    engine::game::zones::move_to_zone(runner.state_mut(), p0_gy_card, Zone::Exile, &mut events);
    assert_eq!(graveyard_len(&runner, P0), 6);
    assert!(
        runner.state().layers_dirty.is_dirty(),
        "re-crossing the seven-card threshold DOWNWARD (a graveyard card leaving via a \
         normal zone move) must also dirty the layer cache"
    );
    assert_eq!(
        effective_pt(&mut runner, mc),
        (3, 3),
        "P0 drops back below the threshold -- only P2's graveyard boost remains (+2/+0)"
    );
}

/// Resolve one draw for `drawer` through the production `draw::resolve` seam,
/// then process the resulting triggers (mirrors the `moon_girl_second_draw_base_pt`
/// / `construct_cosmic_cube_second_draw_token` `draw_one` helper).
fn draw_one(runner: &mut GameRunner, drawer: PlayerId) {
    let ability = ResolvedAbility::new(
        Effect::Draw {
            count: QuantityExpr::Fixed { value: 1 },
            target: TargetFilter::Controller,
        },
        Vec::new(),
        ObjectId(0),
        drawer,
    );
    let mut events = Vec::new();
    resolve_draw(runner.state_mut(), &ability, &mut events).expect("draw resolves");
    process_triggers(runner.state_mut(), &events);
}

/// CR 121.2 + CR 603.3d + CR 701.17: the "draw your second card each turn"
/// trigger fires exactly once per turn, requires a player target when it is
/// put on the stack, and mills the CHOSEN target's library (not the
/// controller's).
#[test]
fn masters_councillors_second_draw_mills_chosen_target_once_per_turn() {
    let mut scenario = GameScenario::new_n_player(3, 42);
    scenario.at_phase(Phase::PreCombatMain);
    scenario.add_creature_from_oracle(P0, "Master's Councillors", 1, 3, MASTERS_COUNCILLORS);

    for i in 0..4 {
        scenario.add_card_to_library_top(P0, &format!("P0 Library {i}"));
    }
    for i in 0..6 {
        scenario.add_card_to_library_top(P1, &format!("P1 Library {i}"));
    }
    let mut runner = scenario.build();

    let p1_library_before = runner.state().players[P1.0 as usize].library.len();
    assert_eq!(graveyard_len(&runner, P1), 0);

    // First draw of the turn: the NthDrawThisTurn=2 gate must NOT fire.
    draw_one(&mut runner, P0);
    assert_eq!(
        runner.state().stack.len(),
        0,
        "first draw must not queue the second-draw trigger"
    );
    // An empty stack alone doesn't prove the trigger never fired -- a fired
    // trigger can sit in `WaitingFor::TriggerTargetSelection` before it ever
    // reaches the stack (see the polling loop below). Asserting `Priority` here
    // is the paired positive reach-guard: it proves the engine is back to a
    // normal priority window, not stalled on a pending target-selection prompt
    // from a trigger that should not have fired.
    assert!(
        matches!(&runner.state().waiting_for, WaitingFor::Priority { .. }),
        "first draw must not leave a pending trigger target-selection prompt"
    );
    assert_eq!(graveyard_len(&runner, P1), 0);

    // Second draw: the trigger fires. Per CR 603.3d its target (target
    // player) must be chosen before it lands on the stack; once it does,
    // priority passes are needed to let it resolve. Drive whichever prompts
    // actually appear (mirrors `orzhov_advokist`'s `resolve_advokist_upkeep`
    // polling loop) rather than assuming one fixed prompt shape/order.
    draw_one(&mut runner, P0);
    let mut targeted = false;
    let mut resolved_something = false;
    for _ in 0..50 {
        match runner.state().waiting_for.clone() {
            WaitingFor::TriggerTargetSelection { .. } | WaitingFor::TargetSelection { .. } => {
                targeted = true;
                runner
                    .act(GameAction::ChooseTarget {
                        target: Some(TargetRef::Player(P1)),
                    })
                    .expect("choosing the mill target succeeds");
            }
            WaitingFor::Priority { .. } if runner.state().stack.is_empty() => break,
            WaitingFor::Priority { .. } => {
                resolved_something = true;
                runner
                    .act(GameAction::PassPriority)
                    .expect("priority progresses the mill trigger");
            }
            other => panic!("unexpected state resolving the second-draw trigger: {other:?}"),
        }
    }
    assert!(
        targeted,
        "CR 603.3d: the mill trigger must present a target-selection prompt for its \
         target player before resolving"
    );
    assert!(
        resolved_something,
        "the targeted trigger must actually reach and pass through a priority window \
         to resolve off the stack"
    );

    // CR 701.17a: the CHOSEN target (P1) mills exactly three cards -- not P0.
    assert_eq!(
        graveyard_len(&runner, P1),
        3,
        "CR 701.17a: the target player mills exactly three cards"
    );
    assert_eq!(
        graveyard_len(&runner, P0),
        0,
        "the controller (P0) is not milled"
    );
    assert_eq!(
        runner.state().players[P1.0 as usize].library.len(),
        p1_library_before - 3,
        "milling moves the top three cards of the TARGET's library, not the controller's"
    );

    // Third draw, same turn: must NOT re-fire -- NthDrawThisTurn is an exact
    // ordinal match (n == 2), not "n >= 2".
    draw_one(&mut runner, P0);
    assert_eq!(
        runner.state().stack.len(),
        0,
        "a third draw the same turn must not re-fire the second-draw trigger"
    );
    assert!(
        matches!(&runner.state().waiting_for, WaitingFor::Priority { .. }),
        "a third draw must not leave a pending trigger target-selection prompt either"
    );
    assert_eq!(
        graveyard_len(&runner, P1),
        3,
        "no additional mill occurs on the third draw"
    );
}
