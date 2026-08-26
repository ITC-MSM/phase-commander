//! Mm'menon, the Right Hand — positive spell-only `NotFrom(Hand)` restriction;
//! and Karolina Dean, Runaway — a narrow prohibition on casts from hand that
//! leaves every non-cast payment context unrestricted.
//!
//! CR 106.6 (restricted mana spend) + CR 400.7 (cast-from zone identity).
//!
//! These tests drive the runtime spend-eligibility decision two ways:
//!   1. `ManaRestriction::allows_spell` — the single authority every payment site
//!      flows through (`PaymentContext::Spell` → `allows_spell`).
//!   2. `ManaPool::spend_for` with `PaymentContext::Spell` — the real mana-payment
//!      route, proving a `NotFrom`-restricted unit is CONSUMED for a spell cast
//!      from a non-hand zone and WITHHELD for a spell cast from hand.
//!
//! Revert-proof: reverting either the polarity axis or Karolina's dedicated
//! prohibition makes the corresponding hand/non-hand and non-cast assertions
//! flip.

use engine::types::identifiers::ObjectId;
use engine::types::mana::{
    ActivationManaColorConstraint, ManaPool, ManaRestriction, ManaType, ManaUnit, PaymentContext,
    SpecialAction, SpellMeta, ZoneSpend, ZoneSpendPolarity,
};
use engine::types::zones::Zone;

/// Mm'menon, the Right Hand: spend only to cast a spell from anywhere other than
/// your hand.
fn not_from_hand_restriction() -> ManaRestriction {
    ManaRestriction::OnlyForSpellFromZone(ZoneSpend {
        zone: Zone::Hand,
        polarity: ZoneSpendPolarity::NotFrom,
    })
}

/// Karolina Dean: this mana cannot pay for the one forbidden cast class, but
/// remains unrestricted for non-cast payments.
fn cannot_cast_from_hand_restriction() -> ManaRestriction {
    ManaRestriction::CannotCastSpellFromZone(Zone::Hand)
}

fn spell_cast_from(zone: Zone) -> SpellMeta {
    SpellMeta {
        types: vec!["Artifact".to_string()],
        cast_from_zone: Some(zone),
        ..SpellMeta::default()
    }
}

#[test]
fn allows_spell_cast_from_non_hand_zone() {
    let r = not_from_hand_restriction();
    // Any cast-from zone except hand qualifies.
    assert!(r.allows_spell(&spell_cast_from(Zone::Graveyard)));
    assert!(r.allows_spell(&spell_cast_from(Zone::Exile)));
    assert!(r.allows_spell(&spell_cast_from(Zone::Library)));
}

#[test]
fn rejects_spell_cast_from_hand() {
    // A normal cast from hand is exactly what this restriction forbids.
    assert!(!not_from_hand_restriction().allows_spell(&spell_cast_from(Zone::Hand)));
}

#[test]
fn rejects_spell_with_unknown_origin() {
    // CR 400.7: a payment site with no associated cast-from zone is ineligible
    // (conservative — never auto-authorize when origin is unknown).
    assert!(!not_from_hand_restriction().allows_spell(&SpellMeta::default()));
}

#[test]
fn never_allows_ability_activation() {
    // CR 106.6: zone-gated spend is spell-casting only.
    assert!(!not_from_hand_restriction().allows_activation(&["Artifact".to_string()], &[], None));
}

/// Drive the REAL mana-payment route: `ManaPool::spend_for` with
/// `PaymentContext::Spell`. A `NotFrom`-restricted unit must be consumed for a
/// non-hand cast and withheld for a hand cast.
#[test]
fn spend_for_consumes_for_non_hand_and_withholds_for_hand() {
    let source = ObjectId(1);
    let make_pool = || {
        let mut pool = ManaPool::default();
        pool.add(ManaUnit::new(
            ManaType::Blue,
            source,
            false,
            vec![not_from_hand_restriction()],
        ));
        pool
    };

    // Eligible: cast from graveyard (non-hand) — the unit is consumed.
    let from_gy = spell_cast_from(Zone::Graveyard);
    let mut pool = make_pool();
    let spent = pool.spend_for(ManaType::Blue, &PaymentContext::Spell(&from_gy));
    assert!(
        spent.is_some(),
        "NotFrom-restricted mana must pay a spell cast from a non-hand zone"
    );
    assert_eq!(pool.total(), 0, "the unit must be consumed");

    // Ineligible: cast from hand — the unit is withheld, pool intact.
    let from_hand = spell_cast_from(Zone::Hand);
    let mut pool = make_pool();
    let spent = pool.spend_for(ManaType::Blue, &PaymentContext::Spell(&from_hand));
    assert!(
        spent.is_none(),
        "NotFrom-restricted mana must not pay a spell cast from hand"
    );
    assert_eq!(pool.total(), 1, "the unit must remain unspent");
}

#[test]
fn karolina_restriction_is_a_narrow_cast_prohibition() {
    let source = ObjectId(1);
    let make_pool = || {
        let mut pool = ManaPool::default();
        pool.add(ManaUnit::new(
            ManaType::White,
            source,
            false,
            vec![cannot_cast_from_hand_restriction()],
        ));
        pool
    };

    let mut hand_pool = make_pool();
    assert!(
        hand_pool
            .spend_for(
                ManaType::White,
                &PaymentContext::Spell(&spell_cast_from(Zone::Hand)),
            )
            .is_none(),
        "Karolina's mana must be withheld from a spell cast from hand"
    );
    assert_eq!(hand_pool.total(), 1);

    for origin in [Zone::Graveyard, Zone::Exile] {
        let mut pool = make_pool();
        assert!(
            pool.spend_for(
                ManaType::White,
                &PaymentContext::Spell(&spell_cast_from(origin)),
            )
            .is_some(),
            "Karolina's mana must pay for a spell cast from {origin:?}"
        );
        assert_eq!(pool.total(), 0, "eligible mana must be consumed");
    }

    let mut unknown_pool = make_pool();
    assert!(
        unknown_pool
            .spend_for(
                ManaType::White,
                &PaymentContext::Spell(&SpellMeta::default()),
            )
            .is_none(),
        "unknown spell origins must fail closed"
    );

    let source_types = ["Creature".to_string()];
    let source_subtypes = ["Human".to_string()];
    let activation = PaymentContext::Activation {
        source_types: &source_types,
        source_subtypes: &source_subtypes,
        ability_tag: None,
        mana_color_constraint: ActivationManaColorConstraint::Unrestricted,
    };
    let mut activation_pool = make_pool();
    assert!(activation_pool
        .spend_for(ManaType::White, &activation)
        .is_some());

    let mut effect_pool = make_pool();
    assert!(effect_pool
        .spend_for(ManaType::White, &PaymentContext::Effect)
        .is_some());

    for action in [
        SpecialAction::CompanionToHand,
        SpecialAction::UnlockDoor,
        SpecialAction::Plot,
        SpecialAction::TurnFaceUp,
        SpecialAction::RollPlanarDie,
        SpecialAction::EndContinuousEffect,
    ] {
        let mut pool = make_pool();
        assert!(
            pool.spend_for(ManaType::White, &PaymentContext::SpecialAction(action))
                .is_some(),
            "Karolina's prohibition must not reject {action:?}"
        );
    }
}

/// Guard against the inclusion polarity regressing: the positive `From` reading
/// must still gate on the named zone (graveyard payable, hand not), proving the
/// polarity axis discriminates both directions from one variant.
#[test]
fn from_polarity_still_gates_inclusively() {
    let from_gy_only = ManaRestriction::OnlyForSpellFromZone(ZoneSpend {
        zone: Zone::Graveyard,
        polarity: ZoneSpendPolarity::From,
    });
    assert!(from_gy_only.allows_spell(&spell_cast_from(Zone::Graveyard)));
    assert!(!from_gy_only.allows_spell(&spell_cast_from(Zone::Hand)));
}
