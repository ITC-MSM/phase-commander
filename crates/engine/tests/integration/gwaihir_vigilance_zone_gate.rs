//! Gwaihir the Windlord — "Other Birds you control have vigilance."
//! Regression coverage for issue #8158.
//!
//! CR 113.6: "Abilities of all other objects usually function only while
//! that object is on the battlefield." A `StaticDefinition` with empty
//! `active_zones` inherits that battlefield-only default
//! (`functioning_abilities::static_functions_in_zone`). Gwaihir carries TWO
//! static abilities that must be gated independently:
//!
//! 1. "This spell costs {2} less to cast..." — `active_zones: [Hand, Stack,
//!    Command, Graveyard, Exile, Library]` (CR 113.6d: a cost-modifying
//!    static functions in every zone the object could be cast from, plus the
//!    stack).
//! 2. "Other Birds you control have vigilance." — empty `active_zones`, so it
//!    must default to battlefield-only.
//!
//! Issue #8158: Gwaihir's cost-reduction static's broad `active_zones`
//! admitted Gwaihir itself into the continuous-effects gather while it sat
//! in Hand, and once admitted, the vigilance-granting static's zone gate
//! was never enforced (an empty `active_zones` skipped the check entirely)
//! — so the vigilance grant leaked into every zone the OTHER static covers,
//! not just the battlefield.
//!
//! Drives the REAL parse -> synthesis -> layer pipeline and reads back the
//! EFFECTIVE post-`evaluate_layers` keyword set on a third-party creature —
//! a runtime test, not an AST-shape test.
//!
//! # Reach-guard (PR #8229 review, MED finding)
//!
//! The negative assertion ("vigilance absent while Gwaihir sits in Hand") is
//! vacuously true if `for_each_static_effect_source`'s off-zone candidate
//! scan never visits Gwaihir at all while it's in Hand — that would also
//! produce "no vigilance," for the wrong reason, and would pass even if the
//! off-zone visitor were deleted entirely. To discriminate "collected from
//! Hand, then correctly filtered by the vigilance static's own empty-
//! `active_zones` gate" from "never collected," a TEST-ONLY marker static is
//! attached to Gwaihir below: it mirrors the SHAPE of Gwaihir's real cost
//! reducer (an opt-in off-zone static with non-empty `active_zones` on the
//! SAME source) but grants an easily observed marker keyword directly to
//! `other_bird`, the same object the real vigilance grant targets. If the
//! marker keyword is present on `other_bird` while Gwaihir sits in Hand, the
//! source WAS visited and its per-definition zone gate WAS evaluated for at
//! least one definition — so the vigilance grant's simultaneous absence is
//! proof of correct per-definition filtering, not a symptom of the source
//! never being reached.
//!
//! This marker is deliberately NOT the real cost-reduction static: that
//! static's `ModifyCost` mode is resolved by
//! `casting::collect_self_spell_cost_modifiers`, which reads the spell
//! object's own `static_definitions` directly and never touches
//! `for_each_static_effect_source` / `static_functions_in_zone` at all — so
//! observing the real cost reduction would not exercise the code path this
//! test is meant to guard. The marker's `Continuous` mode and `SpecificObject`
//! affected-filter route it through the exact same
//! `active_continuous_effects_from_static_source` ->
//! `StaticZoneAdmission::LiveSource` gather the real vigilance grant uses.

use engine::game::keywords::has_keyword;
use engine::game::layers::evaluate_layers;
use engine::game::scenario::{GameRunner, GameScenario, P0};
use engine::game::zones::{add_to_zone, remove_from_zone};
use engine::types::ability::{ContinuousModification, StaticDefinition, TargetFilter};
use engine::types::identifiers::ObjectId;
use engine::types::keywords::Keyword;
use engine::types::phase::Phase;
use engine::types::zones::Zone;

const GWAIHIR_ORACLE: &str = "This spell costs {2} less to cast as long as you've drawn two or more cards this turn.\nFlying, vigilance\nOther Birds you control have vigilance.";

fn has_kw(runner: &mut GameRunner, id: ObjectId, keyword: &Keyword) -> bool {
    runner.state_mut().layers_dirty.mark_full();
    evaluate_layers(runner.state_mut());
    has_keyword(&runner.state().objects[&id], keyword)
}

#[test]
fn gwaihir_vigilance_grant_requires_battlefield_not_hand() {
    let mut scenario = GameScenario::new();
    scenario.at_phase(Phase::PreCombatMain);

    // A second Bird you control, on the battlefield, that WOULD receive the
    // grant if Gwaihir were live. Created first so its id is available to
    // wire into the reach-guard marker below.
    let other_bird = scenario
        .add_creature(P0, "Test Bird", 2, 2)
        .with_subtypes(vec!["Bird"])
        .id();

    // Gwaihir sits in HAND. Its cost-reduction static's broad `active_zones`
    // (Hand among them) admits it to the continuous-effects gather; that
    // must not leak its OTHER static's battlefield-default vigilance grant.
    let gwaihir = scenario
        .add_creature_to_hand_from_oracle(P0, "Gwaihir the Windlord", 4, 4, GWAIHIR_ORACLE)
        .with_subtypes(vec!["Bird", "Noble"])
        // Reach-guard marker (see module doc above) — NOT derived from
        // Gwaihir's Oracle text. Mirrors the real cost reducer's shape
        // (opt-in off-zone, non-empty `active_zones`) so the object is
        // admitted into `for_each_static_effect_source`'s off-zone scan for
        // the SAME reason the real bug needed it to be, then grants a
        // marker keyword to `other_bird` so the visit is independently
        // observable through the exact `has_kw` check already used below.
        .with_static_definition(
            StaticDefinition::continuous()
                .affected(TargetFilter::SpecificObject { id: other_bird })
                .modifications(vec![ContinuousModification::AddKeyword {
                    keyword: Keyword::Reach,
                }])
                .active_zones(vec![Zone::Hand]),
        )
        .id();

    let mut runner = scenario.build();

    assert!(
        has_kw(&mut runner, other_bird, &Keyword::Reach),
        "reach-guard: the marker static (opt-in `active_zones: [Hand]`, same \
         shape as Gwaihir's real cost reducer) must grant its marker keyword \
         from Hand — this proves Gwaihir WAS visited by \
         `for_each_static_effect_source`'s off-zone scan while in hand, so \
         the vigilance assertion below is not vacuously true"
    );
    assert!(
        !has_kw(&mut runner, other_bird, &Keyword::Vigilance),
        "Gwaihir in hand must not grant vigilance to other Birds (CR 113.6: \
         a static with empty `active_zones` defaults to battlefield-only, \
         regardless of what a SIBLING static's `active_zones` declares)"
    );

    // Relocate Gwaihir onto the battlefield directly (mirrors
    // `CardBuilder::with_commander`'s zone-relocation pattern) — no cast or
    // replacement pipeline is exercised here because the bug under test is
    // in continuous-effect gathering, not in casting.
    {
        let state = runner.state_mut();
        let owner = state.objects[&gwaihir].owner;
        remove_from_zone(state, gwaihir, Zone::Hand, owner);
        add_to_zone(state, gwaihir, Zone::Battlefield, owner);
        let ts = state.next_timestamp();
        let obj = state.objects.get_mut(&gwaihir).unwrap();
        obj.zone = Zone::Battlefield;
        obj.timestamp = ts;
    }

    // Positive reach-guard: proves the first assertion isn't vacuously true
    // because the grant never fires at all — the SAME Continuous ability now
    // applies once its zone gate opens.
    assert!(
        has_kw(&mut runner, other_bird, &Keyword::Vigilance),
        "Gwaihir on the battlefield must grant vigilance to other Birds you control"
    );
    // The reach-guard marker's `active_zones: [Hand]` does NOT list
    // Battlefield, so it must stop functioning once Gwaihir relocates —
    // confirms the marker is itself an exact opt-in-zone gate, not a
    // once-admitted-forever leak.
    assert!(
        !has_kw(&mut runner, other_bird, &Keyword::Reach),
        "the Hand-only reach-guard marker must not still apply once Gwaihir \
         is on the battlefield"
    );
}
