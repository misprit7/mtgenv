//! Continuous static enchantments (M5 layer-system pool) plus auras and equipment (#14):
//! anthems, type/P-T changers, and "while attached" buffs over the `AttachedHost`.

use crate::basics::{CardType, Color, Zone};
use crate::cards::helpers::{attached_host, creatures_you_control};
use crate::cards::{aura, enchantment, grp, mana_cost, CardDb, CardDef};
use crate::effects::ability::{
    Ability, Cost, Keyword, Qualification, StaticContribution, Timing,
};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;
use crate::subtypes::ArtifactType;

pub fn register(db: &mut CardDb) {
    // Glorious Anthem {1}{W}{W} — "Creatures you control get +1/+1." (layer 7c ModifyPT.)
    db.insert(enchantment(
        grp::GLORIOUS_ANTHEM,
        "Glorious Anthem",
        Color::White,
        mana_cost(1, &[(Color::White, 2)]),
        vec![Ability::Static {
            contribution: StaticContribution::ModifyPT { power: 1, toughness: 1 },
            affects: creatures_you_control(),
            duration: Duration::WhileSourcePresent,
        }],
    ).with_text("Creatures you control get +1/+1."));
    // Levitation {2}{U}{U} — "Creatures you control have flying." (layer 6 GrantKeyword.)
    db.insert(enchantment(
        grp::LEVITATION,
        "Levitation",
        Color::Blue,
        mana_cost(2, &[(Color::Blue, 2)]),
        vec![Ability::Static {
            contribution: StaticContribution::GrantKeyword(Keyword::Flying),
            affects: creatures_you_control(),
            duration: Duration::WhileSourcePresent,
        }],
    ).with_text("Creatures you control have flying."));
    // Nature's Revolt {3}{G}{G} — "All lands are 2/2 creatures that are still lands." TWO
    // statics: AddType(Creature) (layer 4) + SetBasePT{2,2} (7b), both over all lands. The
    // layer-4 type change is what makes an anthem ("creatures you control") see a land as a
    // creature — the affects-reads-computed (CR 613.8) case.
    let all_lands = || SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::HasCardType(CardType::Land),
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(0),
    };
    db.insert(enchantment(
        grp::NATURES_REVOLT,
        "Nature's Revolt",
        Color::Green,
        mana_cost(3, &[(Color::Green, 2)]),
        vec![
            Ability::Static {
                contribution: StaticContribution::AddType(CardType::Creature),
                affects: all_lands(),
                duration: Duration::WhileSourcePresent,
            },
            Ability::Static {
                contribution: StaticContribution::SetBasePT { power: 2, toughness: 2 },
                affects: all_lands(),
                duration: Duration::WhileSourcePresent,
            },
        ],
    ).with_text("All lands are 2/2 creatures that are still lands."));
    // Bonesplitter {1} Artifact — Equipment. "Equipped creature gets +2/+0. Equip {1}." The
    // static buffs the AttachedHost (layer 7c); equip is a sorcery-speed activated ability that
    // attaches this to a creature you control (CR 301.5 / 702.6).
    db.insert(CardDef {
        chars: Characteristics {
            name: "Bonesplitter".to_string(),
            card_types: vec![CardType::Artifact],
            subtypes: vec![ArtifactType::Equipment.into()],
            colors: Vec::new(),
            mana_cost: Some(mana_cost(1, &[])),
            grp_id: grp::BONESPLITTER,
            ..Default::default()
        },
        abilities: vec![
            Ability::Static {
                contribution: StaticContribution::ModifyPT { power: 2, toughness: 0 },
                affects: attached_host(),
                duration: Duration::WhileSourcePresent,
            },
            Ability::Activated {
                cost: Cost { mana: Some(mana_cost(1, &[])), components: Vec::new() },
                effect: Effect::Attach {
                    what: EffectTarget::SourceSelf,
                    to: EffectTarget::Target(TargetSpec {
                        kind: TargetKind::Creature(CardFilter::ControlledBy(PlayerRef::Controller)),
                        min: 1,
                        max: 1,
                        distinct: true,
                    }),
                },
                timing: Timing::Sorcery,
                restriction: None,
                is_mana: false,
            },
        ],
        text: String::new(),
        fully_implemented: true,
    }.with_text("Equipped creature gets +2/+0. Equip {1}"));
    // Pacifism {1}{W} Aura — "Enchanted creature can't attack or block." Two AttachedHost
    // statics painting the CantAttack/CantBlock qualifications (CR §2.4), read by combat.
    db.insert(aura(
        grp::PACIFISM,
        "Pacifism",
        Color::White,
        mana_cost(1, &[(Color::White, 1)]),
        vec![
            Ability::Static {
                contribution: StaticContribution::Qualification(Qualification::CantAttack),
                affects: attached_host(),
                duration: Duration::WhileSourcePresent,
            },
            Ability::Static {
                contribution: StaticContribution::Qualification(Qualification::CantBlock),
                affects: attached_host(),
                duration: Duration::WhileSourcePresent,
            },
        ],
    ).with_text("Enchant creature. Enchanted creature can't attack or block."));
}
