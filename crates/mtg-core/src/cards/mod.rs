//! Card data — card behaviour as **data** (Characteristics + design's Effect-IR abilities). The
//! core never matches on card names; it interprets these definitions.
//!
//! Organization (per the card-push spec): this module owns the [`CardDef`]/[`CardDb`] types, the
//! card *builders* (`creature`/`spell`/`aura`/…), the `grp::` id constants, and the deck builders.
//! The card *definitions* live in submodules — [`misc`] holds only the basic lands; every other
//! card lives in a `<setcode>/` folder keyed by its first-printing set (one file per card).
//! [`starter_db`] aggregates them all.
//!
//! A [`CardDef`] bundles a card's [`Characteristics`] with its [`Ability`]s; a [`CardDb`] is the
//! registry keyed by `grp_id`. Game objects reference their definition through `chars.grp_id`, so
//! the (non-serializable, fn-pointer-bearing) ability data lives out of the serializable
//! `GameState`. Mana production is first-class Effect IR — `Ability::Activated{is_mana:true}` +
//! `Effect::AddMana` (dorks, conditional/filter lands), or intrinsic basic-land-type mana the
//! engine derives from the computed subtype (CR 305.6); there is no `mana_colors` shortcut.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::basics::{CardType, Color, DamageKind, ManaCost, Zone};
use crate::effects::ability::{Ability, Cost, CostComponent, Keyword, Timing};
use crate::effects::target::{ManaSpec, TargetKind, TargetSpec};
use crate::effects::value::{PlayerRef, ValueExpr};
use crate::effects::{Effect, EffectTarget};
use crate::ids::PlayerId;
use crate::state::{Characteristics, GameState};
use crate::subtypes::{CreatureType, EnchantmentType, Subtype};

/// Shared card-construction fragments (`CardFilter`/`SelectSpec`/`ValueExpr` pieces). Card modules
/// import from here, never from a sibling card module.
pub mod helpers;

pub mod misc;

/// Registered token defs (the reserved 9000+ `grp_id` block) — abilities for tokens that have them.
pub mod tokens;

// Per-first-printing-set folders (real card pool).
pub mod aer;
pub mod ala;
pub mod blb;
pub mod bro;
pub mod dft;
pub mod dsk;
pub mod eld;
pub mod emn;
pub mod eoe;
pub mod fdn;
pub mod fin;
pub mod isd;
pub mod ktk;
pub mod lea;
pub mod m10;
pub mod m12;
pub mod m13;
pub mod mgb;
pub mod mir;
pub mod mkm;
pub mod mrd;
pub mod p02;
pub mod pls;
pub mod por;
pub mod rav;
pub mod rix;
pub mod rtr;
pub mod som;
pub mod sos;
pub mod sth;
pub mod tdm;
pub mod tla;
pub mod tmp;
pub mod tsp;
pub mod ulg;
pub mod usg;
pub mod vow;
pub mod woe;

/// Oracle/printing ids (the `grp_id` linking an object to its [`CardDef`]). Per-set card ids move
/// near their cards in the `<setcode>/` folders; the prototype/starter ids stay here.
pub mod grp {
    pub const PLAINS: u32 = 1;
    pub const ISLAND: u32 = 2;
    pub const MOUNTAIN: u32 = 3;
    pub const FOREST: u32 = 4;
    pub const GRIZZLY_BEARS: u32 = 10;
    pub const HILL_GIANT: u32 = 11;
    pub const SHOCK: u32 = 20;
    pub const DIVINATION: u32 = 21;
    pub const LIGHTNING_BOLT: u32 = 23;
    // M4 prototype cards (triggers + replacement effects).
    pub const ELVISH_VISIONARY: u32 = 30;
    pub const FLAMETONGUE_KAVU: u32 = 31;
    pub const EXULTANT_CULTIST: u32 = 34;
    pub const ROOT_MAZE: u32 = 35;
    pub const HARDENED_SCALES: u32 = 36;
    // M5 layer-system cards (continuous effects).
    pub const GLORIOUS_ANTHEM: u32 = 40;
    pub const LEVITATION: u32 = 41;
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
    pub const BONESPLITTER: u32 = 64;
    pub const PACIFISM: u32 = 65;

    // ── Reserved token-def block (9000+) ──────────────────────────────────────────────────────
    // Registered *token* defs live here, far above organically-growing real-card ids (~260+ and
    // climbing), so they never collide. A `TokenSpec.grp_id` points a created token at one of these
    // so `def_of` supplies its triggered/activated abilities (keywords ride on `TokenSpec.keywords`).
    // These defs carry `Supertype::Token`, so the deck-builder / `/api/cards` catalog filters them out.
    /// 1/1 B/G Pest token — "Whenever this token attacks, you gain 1 life." (SoS Witherbloom Pests).
    pub const PEST_TOKEN: u32 = 9001;
}

/// A card definition: its printed characteristics + abilities (the Effect IR). Card *data*, not
/// game state — holds `Effect` trees, so it is `Debug`/`Clone` but not serde. Mana production is
/// expressed entirely in the IR: an explicit `Ability::Activated{is_mana:true}` (Llanowar, filter
/// lands) or, for basic land types, intrinsic mana the engine derives from the computed subtype
/// (CR 305.6) — there is no separate `mana_colors` shortcut.
#[derive(Debug, Clone, Default)]
pub struct CardDef {
    pub chars: Characteristics,
    pub abilities: Vec<Ability>,
    /// Printed oracle/rules text for display (the view's `rules_text`). Reflects what the
    /// engine actually implements (Scryfall-verified where the implementation matches).
    pub text: String,
    /// Whether the card is implemented faithfully and completely (every printed clause). `true`
    /// by construction for vanilla/keyword/simple cards; set `false` when a genuine subsystem is
    /// deferred (a tracked-incomplete card, e.g. Crew / land-play permissions) — the `text` field
    /// carries the deferral note. Surfaced in the view so the client can flag partial cards.
    pub fully_implemented: bool,
}

impl CardDef {
    /// Builder: set the display rules text.
    pub(crate) fn with_text(mut self, text: &str) -> Self {
        self.text = text.to_string();
        self
    }

    /// Builder: mark this card as tracked-incomplete (a genuine subsystem is deferred). The
    /// builders default `fully_implemented` to `true`; call this on a card that defers a clause.
    /// Currently unused (every authored partial sets the flag inline / is now complete), but kept as
    /// the canonical way to flag a future partial card.
    #[allow(dead_code)]
    pub(crate) fn incomplete(mut self) -> Self {
        self.fully_implemented = false;
        self
    }

    /// The spell ability's effect (CR 113.3a), if this card has one (instants/sorceries).
    pub fn spell_effect(&self) -> Option<&Effect> {
        self.abilities.iter().find_map(|a| match a {
            Ability::Spell { effect } => Some(effect),
            _ => None,
        })
    }
    /// Whether this card has an explicit IR mana ability (`{T}: Add …`). Note: basic-land-type
    /// mana is intrinsic (CR 305.6, derived from the computed subtype) and is NOT reflected here —
    /// this only sees authored `Activated{is_mana}` abilities (Llanowar, filter/conditional lands).
    pub fn is_mana_source(&self) -> bool {
        self.abilities
            .iter()
            .any(|a| matches!(a, Ability::Activated { is_mana: true, .. }))
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
    /// Iterate every registered card as `(grp_id, &CardDef)`, in ascending `grp_id` order (the
    /// `BTreeMap` key order). Lets callers enumerate the **whole pool** — e.g. the lobby deck-builder
    /// catalog and the card-art pipeline — rather than only the cards reachable through deck presets.
    pub fn iter(&self) -> impl Iterator<Item = (u32, &CardDef)> {
        self.defs.iter().map(|(&id, def)| (id, def))
    }
    pub fn len(&self) -> usize {
        self.defs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.defs.is_empty()
    }
}

// ── Card builders (shared by the card-definition submodules) ──────────────────────────────────

pub(crate) fn mana_cost(generic: u32, pips: &[(Color, u32)]) -> ManaCost {
    let mut colored = BTreeMap::new();
    for &(c, n) in pips {
        *colored.entry(c).or_insert(0) += n;
    }
    ManaCost { generic, colored, x: 0, hybrid: Vec::new() }
}

/// As [`mana_cost`] but with two-colour **hybrid** pips (CR 107.4e) — e.g. `{B}{B/G}{G}` is
/// `mana_cost_hybrid(0, &[(Black,1),(Green,1)], &[(Black,Green)])`. Each hybrid pip is payable by
/// either colour.
pub(crate) fn mana_cost_hybrid(
    generic: u32,
    pips: &[(Color, u32)],
    hybrid: &[(Color, Color)],
) -> ManaCost {
    let mut cost = mana_cost(generic, pips);
    cost.hybrid = hybrid.to_vec();
    cost
}

/// A plain `{T}: Add {C}` mana ability (CR 605) as first-class Effect IR — the canonical way to
/// give a *non-basic-type* source its mana (Llanowar, filter lands). The engine reads
/// `Ability::Activated{is_mana:true}` + `Effect::AddMana` to offer the colour and fill the pool.
/// For conditional/any-color/choice mana, build the `Activated` directly with a `restriction` or
/// a richer `ManaSpec` (`any_color` / `one_of`).
pub(crate) fn mana_ability(color: Color) -> Ability {
    Ability::Activated {
        cost: Cost {
            mana: None,
            components: vec![CostComponent::TapSelf],
        },
        effect: Effect::AddMana {
            who: PlayerRef::Controller,
            mana: ManaSpec {
                produces: vec![(color, ValueExpr::Fixed(1))],
                any_color: None,
            },
        },
        timing: Timing::Instant,
        restriction: None,
        is_mana: true,
    }
}

pub(crate) fn basic_land(grp_id: u32, name: &str) -> CardDef {
    let mut chars = Characteristics::basic_land(name);
    chars.grp_id = grp_id;
    chars.colors = Vec::new(); // lands are colorless (CR 105.2a)
    CardDef {
        chars,
        // Mana is intrinsic: the engine derives `{T}: Add <colour>` from the basic land subtype
        // (CR 305.6, e.g. Forest → {G}), so a basic needs no explicit mana ability at all.
        abilities: Vec::new(),
        text: String::new(),
        fully_implemented: true,
    }
}

/// A "check land" / slow-land dual: it enters tapped **unless you control two or more other lands**
/// (evaluated as it enters — it isn't on the battlefield yet, so the `CountAtLeast` counts your
/// *other* lands, matching "two or more other lands"), and taps for one of two colours `a`/`b` via
/// two explicit IR mana abilities (CR 605). Shared by the VOW slow-land cycle (Deathcap Glade,
/// Dreamroot Cascade, Shattered Sanctum, Stormcarved Coast, Sundown Pass).
pub(crate) fn checkland(grp_id: u32, name: &str, a: Color, b: Color) -> CardDef {
    use crate::effects::ability::{ActionPattern, Rewrite};
    use crate::effects::condition::Condition;
    use crate::effects::target::CardFilter;
    let chars = Characteristics {
        name: name.to_string(),
        card_types: vec![CardType::Land],
        grp_id,
        ..Default::default()
    };
    CardDef {
        chars,
        abilities: vec![
            mana_ability(a),
            mana_ability(b),
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersTappedUnless(Condition::CountAtLeast {
                    zone: Zone::Battlefield,
                    filter: CardFilter::HasCardType(CardType::Land),
                    controller: Some(PlayerRef::Controller),
                    n: ValueExpr::Fixed(2),
                }),
            },
        ],
        text: String::new(),
        fully_implemented: true,
    }
}

/// A SoS "surveil dual" tapland: it always enters tapped, taps for one of two colours `a`/`b` (two
/// explicit IR mana abilities), and has a costed `{2}{a}{b}, {T}: Surveil 1` activated ability.
/// Shared by the cycle (Fields of Strife, Forum of Amity, Paradox Gardens, Spectacle Summit,
/// Titan's Grave).
pub(crate) fn surveil_dual(grp_id: u32, name: &str, a: Color, b: Color) -> CardDef {
    use crate::effects::ability::{ActionPattern, Rewrite};
    use crate::effects::target::CardFilter;
    let chars = Characteristics {
        name: name.to_string(),
        card_types: vec![CardType::Land],
        grp_id,
        ..Default::default()
    };
    CardDef {
        chars,
        abilities: vec![
            mana_ability(a),
            mana_ability(b),
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersTapped,
            },
            Ability::Activated {
                cost: Cost {
                    mana: Some(mana_cost(2, &[(a, 1), (b, 1)])),
                    components: vec![CostComponent::TapSelf],
                },
                effect: Effect::Surveil { count: ValueExpr::Fixed(1) },
                timing: Timing::Instant,
                restriction: None,
                is_mana: false,
            },
        ],
        text: String::new(),
        fully_implemented: true,
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn creature(
    grp_id: u32,
    name: &str,
    subtypes: &[CreatureType],
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
            // A creature's subtypes are a *set* (CR 205.3m), no "primary" — pass the full list,
            // e.g. `&[CreatureType::Human, CreatureType::Soldier]`.
            subtypes: subtypes.iter().map(|&s| Subtype::Creature(s)).collect(),
            colors: vec![color],
            mana_cost: Some(cost),
            power: Some(power),
            toughness: Some(toughness),
            grp_id,
            ..Default::default()
        },
        abilities,
        text: String::new(),
        fully_implemented: true,
    }
}

pub(crate) fn vanilla_creature(
    grp_id: u32,
    name: &str,
    subtypes: &[CreatureType],
    color: Color,
    cost: ManaCost,
    power: i32,
    toughness: i32,
) -> CardDef {
    creature(grp_id, name, subtypes, color, cost, power, toughness, Vec::new())
}

/// A creature with printed keyword abilities (CR 702) and no other abilities.
#[allow(clippy::too_many_arguments)]
pub(crate) fn kw_creature(
    grp_id: u32,
    name: &str,
    subtypes: &[CreatureType],
    color: Color,
    cost: ManaCost,
    power: i32,
    toughness: i32,
    keywords: Vec<Keyword>,
) -> CardDef {
    let mut def = creature(grp_id, name, subtypes, color, cost, power, toughness, Vec::new());
    def.chars.keywords = keywords;
    def
}

pub(crate) fn enchantment(grp_id: u32, name: &str, color: Color, cost: ManaCost, abilities: Vec<Ability>) -> CardDef {
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
        text: String::new(),
        fully_implemented: true,
    }
}

/// An Aura (CR 303): an Enchantment with the "Aura" subtype. The engine reads the subtype to
/// require an enchant target at cast and to enter the battlefield attached (CR 303.4f / 608.3e).
pub(crate) fn aura(grp_id: u32, name: &str, color: Color, cost: ManaCost, abilities: Vec<Ability>) -> CardDef {
    let mut def = enchantment(grp_id, name, color, cost, abilities);
    def.chars.subtypes = vec![EnchantmentType::Aura.into()];
    def
}

pub(crate) fn spell(grp_id: u32, name: &str, ty: CardType, color: Color, cost: ManaCost, effect: Effect) -> CardDef {
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
        text: String::new(),
        fully_implemented: true,
    }
}

/// "deal N to any target" (CR 115.4 "any target") — one target, locked at cast.
pub(crate) fn deal_to_any(amount: i64) -> Effect {
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

/// Build the full card registry (all submodules).
pub fn starter_db() -> CardDb {
    let mut db = CardDb::default();
    misc::register(&mut db);
    tokens::register(&mut db);
    // Per-set real cards (Selesnya Landfall push).
    lea::register(&mut db);
    dsk::register(&mut db);
    fin::register(&mut db);
    fdn::register(&mut db);
    eoe::register(&mut db);
    dft::register(&mut db);
    eld::register(&mut db);
    mkm::register(&mut db);
    sos::register(&mut db);
    rav::register(&mut db);
    tla::register(&mut db);
    bro::register(&mut db);
    tdm::register(&mut db);
    blb::register(&mut db);
    vow::register(&mut db);
    woe::register(&mut db);
    // Per-set folders for the prototype/starter pool (moved out of `misc`).
    aer::register(&mut db);
    ala::register(&mut db);
    emn::register(&mut db);
    isd::register(&mut db);
    ktk::register(&mut db);
    m10::register(&mut db);
    m12::register(&mut db);
    m13::register(&mut db);
    mgb::register(&mut db);
    mir::register(&mut db);
    mrd::register(&mut db);
    p02::register(&mut db);
    pls::register(&mut db);
    por::register(&mut db);
    rix::register(&mut db);
    rtr::register(&mut db);
    som::register(&mut db);
    sth::register(&mut db);
    tmp::register(&mut db);
    tsp::register(&mut db);
    ulg::register(&mut db);
    usg::register(&mut db);
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

/// "Heralds": 40 Mist-Cloaked Herald + 20 Island — an intentionally degenerate RL sanity deck
/// where optimal play is trivially "play a land, play creatures, attack with everything." The
/// Herald is a `{U}` 1/1 that can't be blocked, so combat life-loss is unconditional. Preset:
/// `"heralds"`.
pub fn heralds_deck() -> Vec<u32> {
    use std::iter::repeat;
    let mut deck = Vec::new();
    deck.extend(repeat(rix::mist_cloaked_herald::MIST_CLOAKED_HERALD).take(40));
    deck.extend(repeat(grp::ISLAND).take(20));
    deck
}

/// "Swine": 25 Forest + 10 Argothian Swine (`{3}{G}` 3/3 trample) + 25 Grizzly Bears (60) — the
/// user's tier-3 RL sanity deck. A step up from Bears: the Swine's Trample makes chump-blocking
/// leak damage, so optimal play still reduces to "curve out and attack." Preset: `"swine"`.
pub fn swine_deck() -> Vec<u32> {
    use std::iter::repeat;
    let mut deck = Vec::new();
    deck.extend(repeat(grp::FOREST).take(25));
    deck.extend(repeat(grp::ARGOTHIAN_SWINE).take(10));
    deck.extend(repeat(grp::GRIZZLY_BEARS).take(25));
    deck
}

/// The **real** mtggoldfish "Standard Selesnya Landfall" 60 — all 18 distinct nonbasic cards at their
/// decklist quantities + 7 Forest + 2 Plains. Every card is now implemented (most fully; a few as
/// faithful tracked-partials — Mightform's warp, Dyadrine's attack ability, Surrak's stack-spell half),
/// so the basic-land padding that previously stood in for unimplemented cards is gone. Preset:
/// `"selesnya"` / `"landfall"`.
pub fn selesnya_landfall_deck() -> Vec<u32> {
    use std::iter::repeat;
    let mut deck = Vec::new();
    // Nonbasic lands + dorks + landfall payoffs + earthbenders (the implemented cards, at mtggoldfish
    // quantities).
    deck.extend(repeat(eld::fabled_passage::FABLED_PASSAGE).take(4));
    deck.extend(repeat(mkm::escape_tunnel::ESCAPE_TUNNEL).take(4));
    deck.extend(repeat(dsk::hushwood_verge::HUSHWOOD_VERGE).take(4));
    deck.extend(repeat(tla::ba_sing_se::BA_SING_SE).take(3));
    deck.extend(repeat(rav::temple_garden::TEMPLE_GARDEN).take(1));
    deck.extend(repeat(lea::llanowar_elves::LLANOWAR_ELVES).take(4));
    deck.extend(repeat(fin::sazhs_chocobo::SAZHS_CHOCOBO).take(4));
    deck.extend(repeat(fdn::mossborn_hydra::MOSSBORN_HYDRA).take(1));
    deck.extend(repeat(eoe::icetill_explorer::ICETILL_EXPLORER).take(2));
    deck.extend(repeat(dft::lumbering_worldwagon::LUMBERING_WORLDWAGON).take(1));
    deck.extend(repeat(sos::erode::ERODE).take(4));
    deck.extend(repeat(bro::bushwhack::BUSHWHACK).take(2));
    deck.extend(repeat(tdm::surrak_elusive_hunter::SURRAK_ELUSIVE_HUNTER).take(2));
    deck.extend(repeat(tla::badgermole_cub::BADGERMOLE_CUB).take(4));
    deck.extend(repeat(tla::earthbender_ascension::EARTHBENDER_ASCENSION).take(4));
    deck.extend(repeat(eoe::mightform_harmonizer::MIGHTFORM_HARMONIZER).take(4));
    deck.extend(repeat(eoe::dyadrine_synthesis_amalgam::DYADRINE_SYNTHESIS_AMALGAM).take(2));
    deck.extend(repeat(blb::keen_eyed_curator::KEEN_EYED_CURATOR).take(1)); // = 51 (all 18 distinct)
    // The real mtggoldfish basics — no padding left, every nonbasic is implemented.
    deck.extend(repeat(grp::FOREST).take(7));
    deck.extend(repeat(grp::PLAINS).take(2));
    deck
}

/// A preset deck by name (`"burn"`, `"bears"`, `"demo"`, `"heralds"`, `"swine"`,
/// `"selesnya"`/`"landfall"`), case-insensitive. For the harness/CLI/web.
pub fn preset_deck(name: &str) -> Option<Vec<u32>> {
    match name.to_ascii_lowercase().as_str() {
        "burn" => Some(burn_deck()),
        "bears" => Some(bears_deck()),
        "demo" => Some(demo_deck()),
        "heralds" => Some(heralds_deck()),
        "swine" => Some(swine_deck()),
        "selesnya" | "landfall" => Some(selesnya_landfall_deck()),
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
        assert_eq!(db.len(), 153);
        // Forest is "type line only": a Basic Land with subtype Forest. Mana is intrinsic
        // (CR 305.6) — the engine derives {T}: Add {G} from the subtype, so the CardDef carries
        // no explicit mana ability (and `is_mana_source` only sees authored abilities).
        let forest = db.get(grp::FOREST).unwrap();
        assert_eq!(forest.chars.subtypes, vec![crate::subtypes::LandType::Forest.into()]);
        assert!(forest.chars.supertypes.contains(&crate::subtypes::Supertype::Basic));
        assert!(forest.abilities.is_empty());
        assert!(!forest.is_mana_source());
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
    fn card_db_iter_covers_the_whole_pool() {
        let db = starter_db();
        // `iter()` yields every registered card, keyed by its grp_id — count matches `len()`.
        assert_eq!(db.iter().count(), db.len());
        // Ascending grp_id order, and each entry's key matches its def's grp_id.
        let ids: Vec<u32> = db.iter().map(|(id, _)| id).collect();
        assert!(ids.windows(2).all(|w| w[0] < w[1]), "grp_ids are strictly ascending");
        assert!(db.iter().all(|(id, def)| id == def.chars.grp_id), "key == def.grp_id");
        // The catalog reaches SOS cards that no preset deck references (e.g. Wander Off).
        assert!(
            db.iter().any(|(id, _)| id == sos::wander_off::WANDER_OFF),
            "iter() surfaces SOS cards not in any preset deck"
        );
    }

    #[test]
    fn decks_are_the_expected_sizes() {
        assert_eq!(demo_deck().len(), 30);
        assert_eq!(burn_deck().len(), 60);
        assert_eq!(bears_deck().len(), 60);
        assert_eq!(preset_deck("BURN").unwrap().len(), 60);
        assert!(preset_deck("nonesuch").is_none());
        // Selesnya landfall: 60 cards, every one resolves in the DB, both aliases work.
        let db = starter_db();
        let selesnya = selesnya_landfall_deck();
        assert_eq!(selesnya.len(), 60);
        assert!(
            selesnya.iter().all(|&g| db.get(g).is_some()),
            "every Selesnya landfall card resolves in the starter DB"
        );
        assert_eq!(preset_deck("selesnya").unwrap().len(), 60);
        assert_eq!(preset_deck("Landfall").unwrap().len(), 60);
        // Heralds: 60 cards = 40 Mist-Cloaked Herald + 20 Island, all resolving in the DB.
        let heralds = heralds_deck();
        assert_eq!(heralds.len(), 60);
        assert_eq!(
            heralds.iter().filter(|&&g| g == grp::ISLAND).count(),
            20,
            "20 Islands"
        );
        assert_eq!(
            heralds
                .iter()
                .filter(|&&g| g == rix::mist_cloaked_herald::MIST_CLOAKED_HERALD)
                .count(),
            40,
            "40 Mist-Cloaked Heralds"
        );
        assert!(
            heralds.iter().all(|&g| db.get(g).is_some()),
            "every Heralds card resolves in the starter DB"
        );
        assert_eq!(preset_deck("heralds").unwrap().len(), 60);
        assert_eq!(preset_deck("HERALDS").unwrap().len(), 60);
        // Swine (tier-3 RL sanity): 60 = 25 Forest + 10 Argothian Swine + 25 Grizzly Bears, all in the DB.
        let swine = swine_deck();
        assert_eq!(swine.len(), 60);
        assert_eq!(swine.iter().filter(|&&g| g == grp::ARGOTHIAN_SWINE).count(), 10, "10 Argothian Swine");
        assert_eq!(swine.iter().filter(|&&g| g == grp::GRIZZLY_BEARS).count(), 25, "25 Grizzly Bears");
        assert!(swine.iter().all(|&g| db.get(g).is_some()), "every Swine card resolves in the starter DB");
        assert_eq!(preset_deck("swine").unwrap().len(), 60);
        assert_eq!(preset_deck("SWINE").unwrap().len(), 60);
    }
}
