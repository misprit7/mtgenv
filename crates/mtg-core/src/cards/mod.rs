//! The starter card set — card behaviour as **data** (Characteristics + design's Effect-IR
//! abilities). The core never matches on card names; it interprets these definitions.
//!
//! Milestone 3 keeps this tiny (CLAUDE.md "Scope — first pass"): basic lands, two vanilla
//! creatures, a damage instant, a draw sorcery, a gain-life instant. A [`CardDef`] bundles a
//! card's [`Characteristics`] with its [`Ability`]s; a [`CardDb`] is the registry keyed by
//! `grp_id`. Game objects reference their definition through `chars.grp_id`, so the (non-
//! serializable, fn-pointer-bearing) ability data lives out of the serializable `GameState`.
//!
//! Mana abilities are represented engine-side for now (a land "taps for one of these colors")
//! rather than via the full `Ability::Activated{is_mana}` IR — the minimal slice the M3
//! auto-tap payment needs (the IR path stays open).

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::basics::{CardType, Color, CounterKind, DamageKind, ManaCost, Zone};
use crate::effects::ability::{
    Ability, ActionPattern, Cost, CostComponent, EventPattern, Keyword, Qualification, Restriction,
    Rewrite, StaticContribution, Timing,
};
use crate::effects::condition::Duration;
use crate::effects::target::{CardFilter, SelectSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::ids::PlayerId;
use crate::state::{Characteristics, GameState};

/// Oracle/printing ids for the starter set (the `grp_id` linking an object to its [`CardDef`]).
pub mod grp {
    pub const PLAINS: u32 = 1;
    pub const ISLAND: u32 = 2;
    pub const MOUNTAIN: u32 = 3;
    pub const FOREST: u32 = 4;
    pub const GRIZZLY_BEARS: u32 = 10;
    pub const HILL_GIANT: u32 = 11;
    pub const SHOCK: u32 = 20;
    pub const DIVINATION: u32 = 21;
    pub const HEALING_SALVE: u32 = 22;
    pub const LIGHTNING_BOLT: u32 = 23;
    // M4 prototype cards (triggers + replacement effects).
    pub const ELVISH_VISIONARY: u32 = 30;
    pub const FLAMETONGUE_KAVU: u32 = 31;
    pub const SERVANT_OF_THE_SCALE: u32 = 32;
    pub const FOG_BANK: u32 = 33;
    pub const EXULTANT_CULTIST: u32 = 34;
    pub const ROOT_MAZE: u32 = 35;
    pub const HARDENED_SCALES: u32 = 36;
    // M5 layer-system cards (continuous effects).
    pub const GLORIOUS_ANTHEM: u32 = 40;
    pub const LEVITATION: u32 = 41;
    pub const HUMILITY: u32 = 42;
    pub const NATURES_REVOLT: u32 = 43;
    // Evergreen-keyword creatures (#14).
    pub const ELVISH_ARCHERS: u32 = 50;
    pub const FENCING_ACE: u32 = 51;
    pub const ARGOTHIAN_SWINE: u32 = 52;
    pub const TYPHOID_RATS: u32 = 53;
    pub const CHILD_OF_NIGHT: u32 = 54;
    pub const ALABORN_GRENADIER: u32 = 55;
    pub const ALLEY_STRANGLER: u32 = 56;
    pub const WALL_OF_STONE: u32 = 57;
    pub const MURDER: u32 = 58;
    pub const DARKSTEEL_MYR: u32 = 59;
    pub const RAGING_GOBLIN: u32 = 60;
    pub const KING_CHEETAH: u32 = 61;
    pub const GLADECOVER_SCOUT: u32 = 62;
    pub const RANCOR: u32 = 63;
    pub const BONESPLITTER: u32 = 64;
    pub const PACIFISM: u32 = 65;
    pub const CHANDRA_PYROGENIUS: u32 = 66;
}

/// `SelectSpec` for a static affecting "creatures you control" (the anthem scope). min/max are
/// unused for statics (they apply to every match).
fn creatures_you_control() -> SelectSpec {
    SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::All(vec![
            CardFilter::HasCardType(CardType::Creature),
            CardFilter::ControlledBy(PlayerRef::Controller),
        ]),
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(0),
    }
}

/// `SelectSpec` for a static affecting "the permanent this Aura/Equipment is attached to"
/// (CR 702.3e/702.6e) — the source-relative `AttachedHost` filter. min/max unused for statics.
fn attached_host() -> SelectSpec {
    SelectSpec {
        zone: Zone::Battlefield,
        filter: CardFilter::AttachedHost,
        chooser: PlayerRef::Controller,
        min: ValueExpr::Fixed(0),
        max: ValueExpr::Fixed(0),
    }
}

/// A card definition: its printed characteristics + abilities (the Effect IR), plus the
/// engine-side mana colours a land taps for. Card *data*, not game state — holds `Effect`
/// trees, so it is `Debug`/`Clone` but not serde.
#[derive(Debug, Clone)]
pub struct CardDef {
    pub chars: Characteristics,
    pub abilities: Vec<Ability>,
    /// Non-empty ⇒ this permanent has a "{T}: add one mana of one of these colours" ability
    /// (CR 605). Empty for non-mana cards.
    pub mana_colors: Vec<Color>,
    /// Printed oracle/rules text for display (the view's `rules_text`). Reflects what the
    /// engine actually implements (Scryfall-verified where the implementation matches).
    pub text: String,
}

impl CardDef {
    /// Builder: set the display rules text.
    fn with_text(mut self, text: &str) -> Self {
        self.text = text.to_string();
        self
    }

    /// The spell ability's effect (CR 113.3a), if this card has one (instants/sorceries).
    pub fn spell_effect(&self) -> Option<&Effect> {
        self.abilities.iter().find_map(|a| match a {
            Ability::Spell { effect } => Some(effect),
            _ => None,
        })
    }
    pub fn is_mana_source(&self) -> bool {
        !self.mana_colors.is_empty()
    }
}

/// The card registry, keyed by `grp_id`. Default = empty.
#[derive(Debug, Clone, Default)]
pub struct CardDb {
    defs: BTreeMap<u32, CardDef>,
}

impl CardDb {
    pub fn get(&self, grp_id: u32) -> Option<&CardDef> {
        self.defs.get(&grp_id)
    }
    pub fn insert(&mut self, def: CardDef) {
        self.defs.insert(def.chars.grp_id, def);
    }
    pub fn len(&self) -> usize {
        self.defs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

fn mana_cost(generic: u32, pips: &[(Color, u32)]) -> ManaCost {
    let mut colored = BTreeMap::new();
    for &(c, n) in pips {
        *colored.entry(c).or_insert(0) += n;
    }
    ManaCost { generic, colored }
}

fn basic_land(grp_id: u32, name: &str, color: Color) -> CardDef {
    let mut chars = Characteristics::basic_land(name);
    chars.grp_id = grp_id;
    chars.colors = Vec::new(); // lands are colorless (CR 105.2a)
    CardDef {
        chars,
        abilities: Vec::new(),
        mana_colors: vec![color],
        text: String::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn creature(
    grp_id: u32,
    name: &str,
    subtype: &str,
    color: Color,
    cost: ManaCost,
    power: i32,
    toughness: i32,
    abilities: Vec<Ability>,
) -> CardDef {
    CardDef {
        chars: Characteristics {
            name: name.to_string(),
            card_types: vec![CardType::Creature],
            subtypes: vec![subtype.to_string()],
            colors: vec![color],
            mana_cost: Some(cost),
            power: Some(power),
            toughness: Some(toughness),
            grp_id,
            ..Default::default()
        },
        abilities,
        mana_colors: Vec::new(),
        text: String::new(),
    }
}

fn vanilla_creature(
    grp_id: u32,
    name: &str,
    subtype: &str,
    color: Color,
    cost: ManaCost,
    power: i32,
    toughness: i32,
) -> CardDef {
    creature(grp_id, name, subtype, color, cost, power, toughness, Vec::new())
}

/// A creature with printed keyword abilities (CR 702) and no other abilities.
#[allow(clippy::too_many_arguments)]
fn kw_creature(
    grp_id: u32,
    name: &str,
    subtype: &str,
    color: Color,
    cost: ManaCost,
    power: i32,
    toughness: i32,
    keywords: Vec<Keyword>,
) -> CardDef {
    let mut def = creature(grp_id, name, subtype, color, cost, power, toughness, Vec::new());
    def.chars.keywords = keywords;
    def
}

fn enchantment(grp_id: u32, name: &str, color: Color, cost: ManaCost, abilities: Vec<Ability>) -> CardDef {
    CardDef {
        chars: Characteristics {
            name: name.to_string(),
            card_types: vec![CardType::Enchantment],
            colors: vec![color],
            mana_cost: Some(cost),
            grp_id,
            ..Default::default()
        },
        abilities,
        mana_colors: Vec::new(),
        text: String::new(),
    }
}

/// An Aura (CR 303): an Enchantment with the "Aura" subtype. The engine reads the subtype to
/// require an enchant target at cast and to enter the battlefield attached (CR 303.4f / 608.3e).
fn aura(grp_id: u32, name: &str, color: Color, cost: ManaCost, abilities: Vec<Ability>) -> CardDef {
    let mut def = enchantment(grp_id, name, color, cost, abilities);
    def.chars.subtypes = vec!["Aura".to_string()];
    def
}

fn spell(grp_id: u32, name: &str, ty: CardType, color: Color, cost: ManaCost, effect: Effect) -> CardDef {
    CardDef {
        chars: Characteristics {
            name: name.to_string(),
            card_types: vec![ty],
            colors: vec![color],
            mana_cost: Some(cost),
            grp_id,
            ..Default::default()
        },
        abilities: vec![Ability::Spell { effect }],
        mana_colors: Vec::new(),
        text: String::new(),
    }
}

/// "deal N to any target" (CR 115.4 "any target") — one target, locked at cast.
fn deal_to_any(amount: i64) -> Effect {
    Effect::DealDamage {
        amount: ValueExpr::Fixed(amount),
        to: EffectTarget::Target(TargetSpec {
            kind: TargetKind::Any,
            min: 1,
            max: 1,
            distinct: true,
        }),
        kind: DamageKind::Noncombat,
    }
}

/// Build the starter card registry.
pub fn starter_db() -> CardDb {
    let mut db = CardDb::default();
    db.insert(basic_land(grp::PLAINS, "Plains", Color::White).with_text("({T}: Add {W}.)"));
    db.insert(basic_land(grp::ISLAND, "Island", Color::Blue).with_text("({T}: Add {U}.)"));
    db.insert(basic_land(grp::MOUNTAIN, "Mountain", Color::Red).with_text("({T}: Add {R}.)"));
    db.insert(basic_land(grp::FOREST, "Forest", Color::Green).with_text("({T}: Add {G}.)"));
    db.insert(vanilla_creature(
        grp::GRIZZLY_BEARS,
        "Grizzly Bears",
        "Bear",
        Color::Green,
        mana_cost(1, &[(Color::Green, 1)]),
        2,
        2,
    ));
    db.insert(vanilla_creature(
        grp::HILL_GIANT,
        "Hill Giant",
        "Giant",
        Color::Red,
        mana_cost(3, &[(Color::Red, 1)]),
        3,
        3,
    ));
    db.insert(spell(
        grp::SHOCK,
        "Shock",
        CardType::Instant,
        Color::Red,
        mana_cost(0, &[(Color::Red, 1)]),
        deal_to_any(2),
    ).with_text("Shock deals 2 damage to any target."));
    db.insert(spell(
        grp::LIGHTNING_BOLT,
        "Lightning Bolt",
        CardType::Instant,
        Color::Red,
        mana_cost(0, &[(Color::Red, 1)]),
        deal_to_any(3),
    ).with_text("Lightning Bolt deals 3 damage to any target."));
    db.insert(spell(
        grp::DIVINATION,
        "Divination",
        CardType::Sorcery,
        Color::Blue,
        mana_cost(2, &[(Color::Blue, 1)]),
        Effect::Draw {
            who: PlayerRef::Controller,
            count: ValueExpr::Fixed(2),
        },
    ).with_text("Draw two cards."));
    // Simplified to the gain-life mode (the printed card is modal "choose one").
    db.insert(spell(
        grp::HEALING_SALVE,
        "Healing Salve",
        CardType::Instant,
        Color::White,
        mana_cost(0, &[(Color::White, 1)]),
        Effect::GainLife {
            who: PlayerRef::Controller,
            amount: ValueExpr::Fixed(3),
        },
    ).with_text("You gain 3 life."));
    // Elvish Visionary {1}{G} 1/1 — "When this creature enters, draw a card." (ETB trigger.)
    db.insert(creature(
        grp::ELVISH_VISIONARY,
        "Elvish Visionary",
        "Elf Shaman",
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
    ).with_text("When this creature enters, draw a card."));
    // Flametongue Kavu {3}{R} 4/2 — "When this creature enters, it deals 4 damage to target
    // creature." (ETB trigger that targets — chosen as it goes on the stack, CR 603.3d.)
    db.insert(creature(
        grp::FLAMETONGUE_KAVU,
        "Flametongue Kavu",
        "Kavu",
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
                    kind: TargetKind::Creature(crate::effects::target::CardFilter::Any),
                    min: 1,
                    max: 1,
                    distinct: true,
                }),
                kind: DamageKind::Noncombat,
            },
        }],
    ).with_text("When this creature enters, it deals 4 damage to target creature."));
    // Servant of the Scale {G} 0/0 — "This creature enters with a +1/+1 counter on it."
    // (ETB replacement; the dies-trigger clause is omitted for the prototype.) Without the
    // replacement it would be a 0/0 destroyed immediately by the toughness-0 SBA.
    db.insert(creature(
        grp::SERVANT_OF_THE_SCALE,
        "Servant of the Scale",
        "Human Soldier",
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        0,
        0,
        vec![Ability::Replacement {
            // Self-replacement (CR 614.12): only THIS creature, via `ItSelf` (so it doesn't
            // apply to other creatures under the global scan).
            pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
            rewrite: Rewrite::EntersWithCounters {
                kind: CounterKind::PlusOnePlusOne,
                n: 1,
            },
        }],
    ).with_text("This creature enters with a +1/+1 counter on it."));
    // Fog Bank {1}{U} 0/2 — "Prevent all combat damage that would be dealt to and dealt by
    // this creature." (Prototype models the "dealt to" prevention; Defender/Flying and the
    // "dealt by" clause — moot at power 0 — are omitted.)
    db.insert(creature(
        grp::FOG_BANK,
        "Fog Bank",
        "Wall",
        Color::Blue,
        mana_cost(1, &[(Color::Blue, 1)]),
        0,
        2,
        vec![Ability::Replacement {
            // "to THIS creature" — `ItSelf`, so it only prevents damage to Fog Bank itself.
            pattern: ActionPattern::WouldBeDealtDamage {
                to: CardFilter::ItSelf,
                kind: Some(DamageKind::Combat),
            },
            rewrite: Rewrite::Prevent,
        }],
    ).with_text("Prevent all combat damage that would be dealt to this creature."));
    // Exultant Cultist {2}{U} 2/2 — "When this creature dies, draw a card." (dies/LTB trigger.)
    db.insert(creature(
        grp::EXULTANT_CULTIST,
        "Exultant Cultist",
        "Human Wizard",
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
    ).with_text("When this creature dies, draw a card."));
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
    // Humility {2}{W}{W} — "All creatures lose all abilities and have base power and toughness
    // 1/1." Prototype models the base-P/T clause (layer 7b SetBasePT) over ALL creatures; the
    // lose-all-abilities clause (a layer-6 RemoveAllAbilities + its dependency tangle) is
    // deferred — no RemoveAllAbilities contribution in the IR yet.
    db.insert(enchantment(
        grp::HUMILITY,
        "Humility",
        Color::White,
        mana_cost(2, &[(Color::White, 2)]),
        vec![Ability::Static {
            contribution: StaticContribution::SetBasePT { power: 1, toughness: 1 },
            affects: SelectSpec {
                zone: Zone::Battlefield,
                filter: CardFilter::HasCardType(CardType::Creature),
                chooser: PlayerRef::Controller,
                min: ValueExpr::Fixed(0),
                max: ValueExpr::Fixed(0),
            },
            duration: Duration::WhileSourcePresent,
        }],
    ).with_text("All creatures have base power and toughness 1/1. (Lose-all-abilities clause not yet modeled.)"));
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
    // Evergreen-keyword creatures (Scryfall-verified single-keyword bodies).
    db.insert(kw_creature(grp::ELVISH_ARCHERS, "Elvish Archers", "Elf Archer", Color::Green,
        mana_cost(1, &[(Color::Green, 1)]), 2, 1, vec![Keyword::FirstStrike]).with_text("First strike"));
    db.insert(kw_creature(grp::FENCING_ACE, "Fencing Ace", "Human Soldier", Color::White,
        mana_cost(1, &[(Color::White, 1)]), 1, 1, vec![Keyword::DoubleStrike]).with_text("Double strike"));
    db.insert(kw_creature(grp::ARGOTHIAN_SWINE, "Argothian Swine", "Boar", Color::Green,
        mana_cost(3, &[(Color::Green, 1)]), 3, 3, vec![Keyword::Trample]).with_text("Trample"));
    db.insert(kw_creature(grp::TYPHOID_RATS, "Typhoid Rats", "Rat", Color::Black,
        mana_cost(0, &[(Color::Black, 1)]), 1, 1, vec![Keyword::Deathtouch]).with_text("Deathtouch"));
    db.insert(kw_creature(grp::CHILD_OF_NIGHT, "Child of Night", "Vampire", Color::Black,
        mana_cost(1, &[(Color::Black, 1)]), 2, 1, vec![Keyword::Lifelink]).with_text("Lifelink"));
    db.insert(kw_creature(grp::ALABORN_GRENADIER, "Alaborn Grenadier", "Human Soldier", Color::White,
        mana_cost(0, &[(Color::White, 2)]), 2, 2, vec![Keyword::Vigilance]).with_text("Vigilance"));
    db.insert(kw_creature(grp::ALLEY_STRANGLER, "Alley Strangler", "Human Assassin", Color::Black,
        mana_cost(2, &[(Color::Black, 1)]), 2, 3, vec![Keyword::Menace]).with_text("Menace"));
    db.insert(kw_creature(grp::WALL_OF_STONE, "Wall of Stone", "Wall", Color::Red,
        mana_cost(1, &[(Color::Red, 2)]), 0, 8, vec![Keyword::Defender]).with_text("Defender"));
    db.insert(kw_creature(grp::RAGING_GOBLIN, "Raging Goblin", "Goblin", Color::Red,
        mana_cost(0, &[(Color::Red, 1)]), 1, 1, vec![Keyword::Haste]).with_text("Haste"));
    db.insert(kw_creature(grp::KING_CHEETAH, "King Cheetah", "Cat", Color::Green,
        mana_cost(3, &[(Color::Green, 1)]), 3, 2, vec![Keyword::Flash]).with_text("Flash"));
    db.insert(kw_creature(grp::GLADECOVER_SCOUT, "Gladecover Scout", "Elf Scout", Color::Green,
        mana_cost(0, &[(Color::Green, 1)]), 1, 1, vec![Keyword::Hexproof]).with_text("Hexproof"));
    // Darksteel Myr — colorless Artifact Creature, indestructible.
    let mut myr = kw_creature(grp::DARKSTEEL_MYR, "Darksteel Myr", "Myr", Color::White,
        mana_cost(3, &[]), 0, 1, vec![Keyword::Indestructible]);
    myr.chars.card_types = vec![CardType::Artifact, CardType::Creature];
    myr.chars.colors = Vec::new();
    db.insert(myr.with_text("Indestructible"));
    // Murder {1}{B}{B} — "Destroy target creature." (Effect::Destroy.)
    db.insert(spell(
        grp::MURDER,
        "Murder",
        CardType::Instant,
        Color::Black,
        mana_cost(1, &[(Color::Black, 2)]),
        Effect::Destroy {
            what: EffectTarget::Target(TargetSpec {
                kind: TargetKind::Creature(CardFilter::Any),
                min: 1,
                max: 1,
                distinct: true,
            }),
        },
    ).with_text("Destroy target creature."));
    // Rancor {G} Aura — "Enchant creature. Enchanted creature gets +2/+0 and has trample." Two
    // statics over the AttachedHost: layer-7c ModifyPT and layer-6 GrantKeyword(Trample). (The
    // "return to hand when put into a graveyard" recursion clause needs a ReturnToHand effect +
    // dies-trigger for non-creatures — deferred.)
    db.insert(aura(
        grp::RANCOR,
        "Rancor",
        Color::Green,
        mana_cost(0, &[(Color::Green, 1)]),
        vec![
            Ability::Static {
                contribution: StaticContribution::ModifyPT { power: 2, toughness: 0 },
                affects: attached_host(),
                duration: Duration::WhileSourcePresent,
            },
            Ability::Static {
                contribution: StaticContribution::GrantKeyword(Keyword::Trample),
                affects: attached_host(),
                duration: Duration::WhileSourcePresent,
            },
        ],
    ).with_text("Enchant creature. Enchanted creature gets +2/+0 and has trample. (Return-to-hand clause not yet modeled.)"));
    // Bonesplitter {1} Artifact — Equipment. "Equipped creature gets +2/+0. Equip {1}." The
    // static buffs the AttachedHost (layer 7c); equip is a sorcery-speed activated ability that
    // attaches this to a creature you control (CR 301.5 / 702.6).
    db.insert(CardDef {
        chars: Characteristics {
            name: "Bonesplitter".to_string(),
            card_types: vec![CardType::Artifact],
            subtypes: vec!["Equipment".to_string()],
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
        mana_colors: Vec::new(),
        text: String::new(),
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
    // Chandra, Pyrogenius {4}{R}{R} Planeswalker — loyalty 5. Two loyalty abilities (sorcery-
    // speed, once per turn): +2 deals 2 to each opponent; −3 deals 4 to target creature. The
    // −10 ultimate (multi-target sweep) is deferred.
    db.insert(CardDef {
        chars: Characteristics {
            name: "Chandra, Pyrogenius".to_string(),
            card_types: vec![CardType::Planeswalker],
            supertypes: vec!["Legendary".to_string()],
            subtypes: vec!["Chandra".to_string()],
            colors: vec![Color::Red],
            mana_cost: Some(mana_cost(4, &[(Color::Red, 2)])),
            loyalty: Some(5),
            grp_id: grp::CHANDRA_PYROGENIUS,
            ..Default::default()
        },
        abilities: vec![
            Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::Loyalty(2)] },
                effect: Effect::DealDamage {
                    amount: ValueExpr::Fixed(2),
                    to: EffectTarget::Player(PlayerRef::EachOpponent),
                    kind: DamageKind::Noncombat,
                },
                timing: Timing::Sorcery,
                restriction: Some(Restriction::OncePerTurn),
                is_mana: false,
            },
            Ability::Activated {
                cost: Cost { mana: None, components: vec![CostComponent::Loyalty(-3)] },
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
                timing: Timing::Sorcery,
                restriction: Some(Restriction::OncePerTurn),
                is_mana: false,
            },
        ],
        mana_colors: Vec::new(),
        text: String::new(),
    }.with_text("+2: Chandra deals 2 damage to each opponent. −3: Chandra deals 4 damage to target creature. (−10 ultimate not yet modeled.)"));
    db
}

/// A 30-card mono-aggressive Gruul (R/G) demo deck: plenty of mana, vanilla creatures to
/// attack with, and a couple of burn spells — reliably ends a `RandomAgent` game via combat
/// life loss (and exercises casting + the stack + combat + SBAs).
pub fn demo_deck() -> Vec<u32> {
    use std::iter::repeat;
    let mut deck = Vec::new();
    deck.extend(repeat(grp::FOREST).take(11));
    deck.extend(repeat(grp::MOUNTAIN).take(11));
    deck.extend(repeat(grp::GRIZZLY_BEARS).take(4));
    deck.extend(repeat(grp::HILL_GIANT).take(2));
    deck.extend(repeat(grp::SHOCK).take(2));
    deck
}

/// "Burn": 40 Lightning Bolt + 20 Mountain (CR — a mono-red burn deck). Exercises
/// instant-speed casting, "any target" (face or creature), and the `DealDamage` runtime.
pub fn burn_deck() -> Vec<u32> {
    use std::iter::repeat;
    let mut deck = Vec::new();
    deck.extend(repeat(grp::LIGHTNING_BOLT).take(40));
    deck.extend(repeat(grp::MOUNTAIN).take(20));
    deck
}

/// "Bears": 40 Grizzly Bears + 20 Forest. Exercises sorcery-speed creature casting and
/// combat (attack / block / damage / lethal-damage SBA).
pub fn bears_deck() -> Vec<u32> {
    use std::iter::repeat;
    let mut deck = Vec::new();
    deck.extend(repeat(grp::GRIZZLY_BEARS).take(40));
    deck.extend(repeat(grp::FOREST).take(20));
    deck
}

/// A preset deck by name (`"burn"`, `"bears"`, `"demo"`), case-insensitive. For the harness/CLI.
pub fn preset_deck(name: &str) -> Option<Vec<u32>> {
    match name.to_ascii_lowercase().as_str() {
        "burn" => Some(burn_deck()),
        "bears" => Some(bears_deck()),
        "demo" => Some(demo_deck()),
        _ => None,
    }
}

/// Build a two-player game from the demo deck with the starter [`CardDb`] attached.
pub fn two_player_demo_game(seed: u64) -> GameState {
    build_game(seed, &[&demo_deck(), &demo_deck()])
}

/// The user's hand-test matchup: seat 0 plays Burn, seat 1 plays Bears.
pub fn burn_vs_bears_game(seed: u64) -> GameState {
    build_game(seed, &[&burn_deck(), &bears_deck()])
}

/// Build a game: one library per seat from a list of `grp_id` decks, with `starter_db()`
/// attached. Cards are added to libraries in deck order (the engine shuffles at game start).
pub fn build_game(seed: u64, decks: &[&[u32]]) -> GameState {
    let mut state = GameState::new(decks.len(), seed);
    state.set_card_db(Arc::new(starter_db()));
    let db = state.card_db();
    for (seat, deck) in decks.iter().enumerate() {
        let cards: Vec<Characteristics> = deck
            .iter()
            .filter_map(|&g| db.get(g).map(|d| d.chars.clone()))
            .collect();
        for chars in cards {
            state.add_card(PlayerId(seat as u32), chars, Zone::Library);
        }
    }
    state
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starter_db_has_expected_cards() {
        let db = starter_db();
        assert_eq!(db.len(), 38);
        assert!(db.get(grp::FOREST).unwrap().is_mana_source());
        assert_eq!(db.get(grp::FOREST).unwrap().mana_colors, vec![Color::Green]);
        // Grizzly Bears is a vanilla 2/2 with no abilities.
        let bears = db.get(grp::GRIZZLY_BEARS).unwrap();
        assert_eq!(bears.chars.power, Some(2));
        assert!(bears.abilities.is_empty());
        assert!(!bears.is_mana_source());
        // Shock and Lightning Bolt are instants with a spell ability.
        assert!(db.get(grp::SHOCK).unwrap().spell_effect().is_some());
        assert!(db.get(grp::LIGHTNING_BOLT).unwrap().spell_effect().is_some());
        // Oracle text is carried for display (the view's rules_text); vanilla cards have none.
        assert_eq!(
            db.get(grp::LIGHTNING_BOLT).unwrap().text,
            "Lightning Bolt deals 3 damage to any target."
        );
        assert!(db.get(grp::GRIZZLY_BEARS).unwrap().text.is_empty());
    }

    #[test]
    fn decks_are_the_expected_sizes() {
        assert_eq!(demo_deck().len(), 30);
        assert_eq!(burn_deck().len(), 60);
        assert_eq!(bears_deck().len(), 60);
        assert_eq!(preset_deck("BURN").unwrap().len(), 60);
        assert!(preset_deck("nonesuch").is_none());
    }
}
