//! Registered **token defs** — the reserved 9000+ `grp_id` block (see [`super::grp`]). A token created
//! from a [`TokenSpec`](crate::effects::target::TokenSpec) with a nonzero `grp_id` points at one of
//! these defs, so `def_of` supplies its **triggered/activated abilities** (keywords ride on the spec).
//! Each def carries `Supertype::Token`, so the deck-builder / `/api/cards` catalog filters it out.
//!
//! Card-agnostic law: a token's behaviour is still *data* (this def's `Ability` list), never a
//! name-match in the core.

use crate::basics::{CardType, Color};
use crate::cards::helpers::{attached_host, sacrifice_self};
use crate::cards::{grp, mana_cost, CardDb, CardDef};
use crate::effects::ability::{
    Ability, Cost, CostComponent, EventPattern, Keyword, StaticContribution, Timing,
};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, ManaSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;
use crate::subtypes::{ArtifactType, CreatureType, EnchantmentType, Supertype};

pub fn register(db: &mut CardDb) {
    // 1/1 black-and-green Pest — "Whenever this token attacks, you gain 1 life." (SoS Witherbloom).
    db.insert(CardDef {
        chars: Characteristics {
            name: "Pest".to_string(),
            card_types: vec![CardType::Creature],
            subtypes: vec![CreatureType::Pest.into()],
            supertypes: vec![Supertype::Token],
            colors: vec![Color::Black, Color::Green],
            power: Some(1),
            toughness: Some(1),
            grp_id: grp::PEST_TOKEN,
            ..Default::default()
        },
        abilities: vec![Ability::Triggered {
            event: EventPattern::SelfAttacks,
            condition: None,
            intervening_if: false,
            effect: Effect::GainLife { who: PlayerRef::Controller, amount: ValueExpr::Fixed(1) },
        }],
        text: "Whenever this token attacks, you gain 1 life.".to_string(),
        fully_implemented: true,
    });

    // Treasure — colourless artifact token: "{T}, Sacrifice this token: Add one mana of any color."
    // (CR 111.3 / Treasure). A cost-bearing mana ability (the sacrifice) — usable only via manual mana
    // activation, kept out of the auto-pay pool (`mana::mana_sources_kind` skips non-`{T}` mana costs).
    db.insert(CardDef {
        chars: Characteristics {
            name: "Treasure".to_string(),
            card_types: vec![CardType::Artifact],
            subtypes: vec![ArtifactType::Treasure.into()],
            supertypes: vec![Supertype::Token],
            colors: vec![], // colourless
            grp_id: grp::TREASURE_TOKEN,
            ..Default::default()
        },
        abilities: vec![Ability::Activated {
            cost: Cost {
                mana: None,
                components: vec![CostComponent::TapSelf, CostComponent::Sacrifice(sacrifice_self())],
            },
            effect: Effect::AddMana {
                who: PlayerRef::Controller,
                mana: ManaSpec { produces: vec![], any_color: Some(ValueExpr::Fixed(1)), one_of: None, restriction: None },
            },
            timing: Timing::Instant,
            restriction: None,
            is_mana: true,
        }],
        text: "{T}, Sacrifice this token: Add one mana of any color.".to_string(),
        fully_implemented: true,
    });

    // Clue — colourless artifact token: "{2}, Sacrifice this token: Draw a card." (CR 111.3 / Clue,
    // Investigate). A non-mana activated ability offered at priority like any other.
    db.insert(CardDef {
        chars: Characteristics {
            name: "Clue".to_string(),
            card_types: vec![CardType::Artifact],
            subtypes: vec![ArtifactType::Clue.into()],
            supertypes: vec![Supertype::Token],
            colors: vec![], // colourless
            grp_id: grp::CLUE_TOKEN,
            ..Default::default()
        },
        abilities: vec![Ability::Activated {
            cost: Cost {
                mana: Some(crate::cards::mana_cost(2, &[])),
                components: vec![CostComponent::Sacrifice(sacrifice_self())],
            },
            effect: Effect::Draw { who: PlayerRef::Controller, count: ValueExpr::Fixed(1) },
            timing: Timing::Instant,
            restriction: None,
            is_mana: false,
        }],
        text: "{2}, Sacrifice this token: Draw a card.".to_string(),
        fully_implemented: true,
    });

    // Monster Role — Enchantment — Aura Role token: "Enchanted creature gets +1/+1 and has trample."
    // (Monstrous Rage.) Two host-scoped statics (the Pacifism/Bonesplitter idiom): +1/+1 in layer 7c,
    // trample in layer 6. Roles are colourless (CR 111.10) auras.
    db.insert(role_token_def(
        grp::MONSTER_ROLE_TOKEN,
        "Monster Role",
        vec![
            role_static(StaticContribution::ModifyPT { power: 1, toughness: 1 }),
            role_static(StaticContribution::GrantKeyword(Keyword::Trample)),
        ],
        "Enchanted creature gets +1/+1 and has trample.",
    ));

    // Royal Role — Enchantment — Aura Role token: "Enchanted creature gets +1/+1 and has ward {1}."
    // (Royal Treatment.) +1/+1 static + Ward {1} as a printed `BecomesTargeted{AttachedHost}` trigger
    // ON the token (bare `Keyword::Ward` carries no cost; the trigger IS the ward, CR 702.21). Reads its
    // host via the `AttachedHost` filter, so it fires when the enchanted creature is targeted by an
    // opponent and counters that spell/ability unless they pay {1}.
    db.insert(role_token_def(
        grp::ROYAL_ROLE_TOKEN,
        "Royal Role",
        vec![
            role_static(StaticContribution::ModifyPT { power: 1, toughness: 1 }),
            Ability::Triggered {
                event: EventPattern::BecomesTargeted { filter: CardFilter::AttachedHost, by_opponent: true },
                condition: None,
                intervening_if: false,
                effect: Effect::CounterUnlessPay {
                    what: EffectTarget::Triggering,
                    cost: Cost { mana: Some(mana_cost(1, &[])), components: Vec::new() },
                },
            },
        ],
        "Enchanted creature gets +1/+1 and has ward {1}.",
    ));
}

/// A host-scoped static contribution for a Role Aura token (affects the enchanted creature while the
/// Role is present) — the Pacifism/Bonesplitter idiom.
fn role_static(contribution: StaticContribution) -> Ability {
    Ability::Static { contribution, affects: attached_host(), duration: Duration::WhileSourcePresent }
}

/// Build a registered **Role Aura token** def (Enchantment — Aura Role, colourless, `Supertype::Token`)
/// carrying `abilities` and pointing at `grp_id` so `def_of` finds them. Minted attached to a creature
/// by [`crate::effects::Effect::CreateRoleToken`].
fn role_token_def(grp_id: u32, name: &str, abilities: Vec<Ability>, text: &str) -> CardDef {
    CardDef {
        chars: Characteristics {
            name: name.to_string(),
            card_types: vec![CardType::Enchantment],
            subtypes: vec![EnchantmentType::Aura.into(), EnchantmentType::Role.into()],
            supertypes: vec![Supertype::Token],
            colors: vec![], // Roles are colourless (CR 111.10)
            grp_id,
            ..Default::default()
        },
        abilities,
        text: text.to_string(),
        fully_implemented: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pest_token_def_is_registered_and_token_supertyped() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::PEST_TOKEN).unwrap();
        assert!(def.chars.supertypes.contains(&Supertype::Token), "Token supertype → excluded from the catalog");
        assert!(matches!(def.abilities[0], Ability::Triggered { event: EventPattern::SelfAttacks, .. }));
    }

    /// CR 111.7 token cease-to-exist, verified end-to-end (the three lead requirements):
    /// (1) a token's death still fires "when a creature you control dies" watchers, reading the dead
    ///     token's LKI (Cauldron of Essence drains 1 — an Essenceknit-class watcher);
    /// (2) the cease reuses `cease_to_exist` (not a second removal), so the token is GONE (phantom-gy
    ///     regression: a dead token does NOT linger in the graveyard);
    /// (3) the cease does NOT register as "a card left the graveyard" — Kirol/Ark-class leave-gy
    ///     watchers must NOT fire (`cards_left_graveyard_this_turn` stays 0), the inverse of the bug.
    #[test]
    fn dead_token_fires_dies_triggers_then_ceases_without_a_leave_graveyard() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::{DamageKind, Target, Zone};
        use crate::cards::{build_game, grp};
        use crate::ids::PlayerId;
        use crate::priority::Engine;

        #[derive(Clone)]
        struct Passive;
        impl Agent for Passive {
            fn decide(&mut self, _v: &PlayerView, _r: &DecisionRequest) -> DecisionResponse {
                DecisionResponse::Pass
            }
        }

        let mut state = build_game(1, &[&[], &[]]);
        // P0 controls a Cauldron of Essence (dies-drain watcher) and a Pest TOKEN (1/1).
        let cauldron_grp = crate::cards::sos::cauldron_of_essence::CAULDRON_OF_ESSENCE;
        let cauldron = state.add_card(PlayerId(0), state.card_db().get(cauldron_grp).unwrap().chars.clone(), Zone::Battlefield);
        let pest = state.add_card(PlayerId(0), state.card_db().get(grp::PEST_TOKEN).unwrap().chars.clone(), Zone::Battlefield);
        assert!(state.object(pest).chars.supertypes.contains(&Supertype::Token));
        let life0 = state.player(PlayerId(0)).life;
        let opp0 = state.player(PlayerId(1)).life;
        let mut e = Engine::new(state, vec![Box::new(Passive), Box::new(Passive)]);
        // Lethal damage to the 1/1 Pest → CreatureDies SBA → graveyard → TokenCeasesToExist.
        e.apply_damage(Target::Object(pest), 1, cauldron, DamageKind::Combat);
        e.run_agenda();
        while !e.state.stack.items.is_empty() {
            e.resolve_top();
            e.run_agenda();
        }
        // (1) dies-trigger fired (Cauldron drained: P0 +1, P1 −1) — reads the dead token's LKI controller.
        assert_eq!(e.state.player(PlayerId(0)).life, life0 + 1, "Cauldron's dies-drain fired for the token");
        assert_eq!(e.state.player(PlayerId(1)).life, opp0 - 1, "opponent lost 1 to the drain");
        // (2) the token ceased — not lingering in any graveyard (phantom-gy regression).
        assert!(!e.state.objects.contains_key(&pest), "the dead token ceased to exist (CR 111.7)");
        assert!(!e.state.player(PlayerId(0)).graveyard.contains(&pest), "not a phantom in the graveyard");
        // (3) the cease is NOT a graveyard-leave — leave-gy watchers/counters must not see it.
        assert_eq!(
            e.state.player(PlayerId(0)).cards_left_graveyard_this_turn, 0,
            "token cease bypasses move_object → no phantom leave-graveyard (Kirol/Ark stay silent)"
        );
    }
}
