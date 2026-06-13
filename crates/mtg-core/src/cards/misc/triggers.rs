//! Triggered-ability and replacement-effect cards — the M4 prototype pool (ETB/dies triggers,
//! self- and global replacement effects).

use crate::basics::{CardType, Color, CounterKind, DamageKind};
use crate::cards::{creature, enchantment, grp, mana_cost, CardDb};
use crate::effects::ability::{Ability, ActionPattern, EventPattern, Rewrite};
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::subtypes::CreatureType;

pub fn register(db: &mut CardDb) {
    // Elvish Visionary {1}{G} 1/1 — "When this creature enters, draw a card." (ETB trigger.)
    let mut visionary = creature(
        grp::ELVISH_VISIONARY,
        "Elvish Visionary",
        CreatureType::Elf,
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        1,
        1,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::Draw {
                who: PlayerRef::Controller,
                count: ValueExpr::Fixed(1),
            },
        }],
    );
    visionary.chars.subtypes = vec![CreatureType::Elf.into(), CreatureType::Shaman.into()];
    db.insert(visionary.with_text("When this creature enters, draw a card."));
    // Flametongue Kavu {3}{R} 4/2 — "When this creature enters, it deals 4 damage to target
    // creature." (ETB trigger that targets — chosen as it goes on the stack, CR 603.3d.)
    db.insert(creature(
        grp::FLAMETONGUE_KAVU,
        "Flametongue Kavu",
        CreatureType::Kavu,
        Color::Red,
        mana_cost(3, &[(Color::Red, 1)]),
        4,
        2,
        vec![Ability::Triggered {
            event: EventPattern::SelfEnters,
            condition: None,
            intervening_if: false,
            effect: Effect::DealDamage {
                amount: ValueExpr::Fixed(4),
                to: EffectTarget::Target(TargetSpec {
                    kind: TargetKind::Creature(CardFilter::Any),
                    min: 1,
                    max: 1,
                    distinct: true,
                }),
                kind: DamageKind::Noncombat,
            },
        }],
    ).with_text("When this creature enters, it deals 4 damage to target creature."));
    // Exultant Cultist {2}{U} 2/2 — "When this creature dies, draw a card." (dies/LTB trigger.)
    let mut cultist = creature(
        grp::EXULTANT_CULTIST,
        "Exultant Cultist",
        CreatureType::Human,
        Color::Blue,
        mana_cost(2, &[(Color::Blue, 1)]),
        2,
        2,
        vec![Ability::Triggered {
            event: EventPattern::SelfDies,
            condition: None,
            intervening_if: false,
            effect: Effect::Draw {
                who: PlayerRef::Controller,
                count: ValueExpr::Fixed(1),
            },
        }],
    );
    cultist.chars.subtypes = vec![CreatureType::Human.into(), CreatureType::Wizard.into()];
    db.insert(cultist.with_text("When this creature dies, draw a card."));
    // Root Maze {G} Enchantment — "Artifacts and lands enter tapped." (GLOBAL replacement,
    // affects all players' artifacts/lands.)
    db.insert(enchantment(
        grp::ROOT_MAZE,
        "Root Maze",
        Color::Green,
        mana_cost(1, &[]),
        vec![Ability::Replacement {
            pattern: ActionPattern::WouldEnterBattlefield(CardFilter::AnyOf(vec![
                CardFilter::HasCardType(CardType::Artifact),
                CardFilter::HasCardType(CardType::Land),
            ])),
            rewrite: Rewrite::EntersTapped,
        }],
    ).with_text("Artifacts and lands enter tapped."));
    // Hardened Scales {G} Enchantment — "If one or more +1/+1 counters would be put on a
    // creature you control, that many plus one are put on it instead." (GLOBAL counter
    // modifier scoped to creatures the controller controls.)
    db.insert(enchantment(
        grp::HARDENED_SCALES,
        "Hardened Scales",
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        vec![Ability::Replacement {
            pattern: ActionPattern::WouldAddCounters {
                kind: CounterKind::PlusOnePlusOne,
                to: CardFilter::ControlledBy(PlayerRef::Controller),
            },
            rewrite: Rewrite::AddAmount(1),
        }],
    ).with_text("If one or more +1/+1 counters would be put on a creature you control, that many plus one +1/+1 counters are put on it instead."));
}
