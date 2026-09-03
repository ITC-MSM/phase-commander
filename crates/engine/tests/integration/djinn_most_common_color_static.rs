//! Sulam Djinn (Invasion) — "Trample\nThis creature gets -2/-2 as long as
//! green is the most common color among all permanents or is tied for most
//! common."
//!
//! Regression coverage for the shared "[color] is the most common color among
//! all permanents [or is tied for most common]" static condition that gates
//! the whole Invasion Djinn cycle (Sulam Djinn — green, Goham Djinn — black,
//! Ruham Djinn — white, Zanam Djinn — blue, Halam Djinn — red). Axes:
//!   - **majority gate** — the -2/-2 applies while the named color strictly
//!     leads the battlefield-wide color census (CR 105.2 + CR 611.3a),
//!   - **minority gate** — the -2/-2 does NOT apply while another color
//!     strictly leads,
//!   - **tie gate** — "or is tied for most common" means a tie for the lead
//!     STILL satisfies the condition (`Comparator::GE`, not strict `GT`).
//!
//! Drives the REAL parse → synthesis → layer pipeline and reads back the
//! EFFECTIVE post-`evaluate_layers` power/toughness — a runtime test, not an
//! AST-shape test.

use engine::game::layers::evaluate_layers;
use engine::game::scenario::{GameRunner, GameScenario, P0, P1};
use engine::types::identifiers::ObjectId;
use engine::types::mana::{ManaCost, ManaCostShard};
use engine::types::phase::Phase;

const SULAM_DJINN: &str = "Trample\nThis creature gets -2/-2 as long as green is the most common color among all permanents or is tied for most common.";

fn effective_pt(runner: &mut GameRunner, id: ObjectId) -> (i32, i32) {
    runner.state_mut().layers_dirty.mark_full();
    evaluate_layers(runner.state_mut());
    let obj = &runner.state().objects[&id];
    (
        obj.power.expect("creature has power"),
        obj.toughness.expect("creature has toughness"),
    )
}

fn green_cost() -> ManaCost {
    ManaCost::Cost {
        generic: 0,
        shards: vec![ManaCostShard::Green],
    }
}

fn blue_cost() -> ManaCost {
    ManaCost::Cost {
        generic: 0,
        shards: vec![ManaCostShard::Blue],
    }
}

/// CR 105.2 + CR 611.3a: green strictly leads the battlefield-wide color
/// census (Sulam Djinn itself plus two more green permanents outnumber a
/// single blue permanent) — the -2/-2 applies.
#[test]
fn sulam_djinn_shrinks_when_green_is_strictly_most_common() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    let sulam = scenario
        .add_creature_from_oracle(P0, "Sulam Djinn", 6, 6, SULAM_DJINN)
        .with_mana_cost(green_cost())
        .id();
    // Two more green permanents: Sulam Djinn (1) + these (2) = 3 green.
    scenario
        .add_creature(P0, "Green Filler A", 1, 1)
        .with_mana_cost(green_cost());
    scenario
        .add_creature(P0, "Green Filler B", 1, 1)
        .with_mana_cost(green_cost());
    // A single blue permanent: 1 blue, strictly fewer than 3 green.
    scenario
        .add_creature(P1, "Blue Filler", 1, 1)
        .with_mana_cost(blue_cost());

    let mut runner = scenario.build();

    assert_eq!(
        effective_pt(&mut runner, sulam),
        (4, 4),
        "green strictly leads (3 green vs 1 blue): -2/-2 must apply (6/6 base -> 4/4)"
    );
}

/// CR 105.2 + CR 611.3a: blue strictly leads the battlefield-wide color
/// census (more blue permanents than green) — the -2/-2 must NOT apply.
#[test]
fn sulam_djinn_stays_base_when_another_color_is_strictly_most_common() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    let sulam = scenario
        .add_creature_from_oracle(P0, "Sulam Djinn", 6, 6, SULAM_DJINN)
        .with_mana_cost(green_cost())
        .id();
    // Total green count: Sulam Djinn (1) only.
    // Blue count: 3 — strictly more than green.
    scenario
        .add_creature(P1, "Blue Filler A", 1, 1)
        .with_mana_cost(blue_cost());
    scenario
        .add_creature(P1, "Blue Filler B", 1, 1)
        .with_mana_cost(blue_cost());
    scenario
        .add_creature(P1, "Blue Filler C", 1, 1)
        .with_mana_cost(blue_cost());

    let mut runner = scenario.build();

    assert_eq!(
        effective_pt(&mut runner, sulam),
        (6, 6),
        "blue strictly leads (1 green vs 3 blue): -2/-2 must NOT apply, Sulam Djinn stays base 6/6"
    );
}

/// CR 105.2 + CR 611.3a: green is TIED for most common (equal counts of green
/// and blue permanents) — "or is tied for most common" means the -2/-2 STILL
/// applies; a tie is not a strict-minority escape.
#[test]
fn sulam_djinn_shrinks_when_green_is_tied_for_most_common() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    let sulam = scenario
        .add_creature_from_oracle(P0, "Sulam Djinn", 6, 6, SULAM_DJINN)
        .with_mana_cost(green_cost())
        .id();
    // One more green permanent: Sulam Djinn (1) + this (1) = 2 green.
    scenario
        .add_creature(P0, "Green Filler", 1, 1)
        .with_mana_cost(green_cost());
    // Exactly 2 blue permanents — tied with green, not strictly greater.
    scenario
        .add_creature(P1, "Blue Filler A", 1, 1)
        .with_mana_cost(blue_cost());
    scenario
        .add_creature(P1, "Blue Filler B", 1, 1)
        .with_mana_cost(blue_cost());

    let mut runner = scenario.build();

    assert_eq!(
        effective_pt(&mut runner, sulam),
        (4, 4),
        "green is tied for most common (2 green, 2 blue): -2/-2 must still apply per \
         \"or is tied for most common\" (6/6 base -> 4/4)"
    );
}
