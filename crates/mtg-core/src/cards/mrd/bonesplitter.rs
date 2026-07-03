//! Bonesplitter — `{1}` Artifact — Equipment. "Equipped creature gets +2/+0. Equip {1}." The
//! static buffs the AttachedHost (layer 7c); equip is a sorcery-speed activated ability that
//! attaches this to a creature you control (CR 301.5 / 702.6). First printed MRD (Mirrodin).

use crate::basics::CardType;
use crate::cards::helpers::attached_host;
use crate::cards::{grp, mana_cost, CardDb, CardDef};
use crate::effects::ability::{Ability, Cost, StaticContribution, Timing};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, TargetKind, TargetSpec};
use crate::effects::value::PlayerRef;
use crate::effects::{Effect, EffectTarget};
use crate::state::Characteristics;
use crate::subtypes::ArtifactType;

pub fn register(db: &mut CardDb) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::expect;

    #[test]
    fn bonesplitter_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(grp::BONESPLITTER).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Artifact]);
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Static {
                    contribution: ModifyPT {
                        power: 2,
                        toughness: 0,
                    },
                    affects: SelectSpec {
                        zone: Battlefield,
                        filter: AttachedHost,
                        chooser: Controller,
                        min: Fixed(
                            0,
                        ),
                        max: Fixed(
                            0,
                        ),
                    },
                    duration: WhileSourcePresent,
                },
                Activated {
                    cost: Cost {
                        mana: Some(
                            ManaCost {
                                generic: 1,
                                colored: {},
                                x: 0,
                                hybrid: [],
                            },
                        ),
                        components: [],
                    },
                    effect: Attach {
                        what: SourceSelf,
                        to: Target(
                            TargetSpec {
                                kind: Creature(
                                    ControlledBy(
                                        Controller,
                                    ),
                                ),
                                min: 1,
                                max: 1,
                                distinct: true,
                            },
                        ),
                    },
                    timing: Sorcery,
                    restriction: None,
                    is_mana: false,
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }
}
