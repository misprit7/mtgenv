//! Dyadrine, Synthesis Amalgam — `{X}{G}{W}` Legendary Artifact Creature — Construct 0/1 (first
//! printed EOE, Edge of Eternities).
//!
//! Oracle:
//!   Trample
//!   Dyadrine enters with a number of +1/+1 counters on it equal to the amount of mana spent to cast
//!   it.
//!   Whenever you attack, you may remove a +1/+1 counter from each of two creatures you control. If
//!   you do, draw a card and create a 2/2 colorless Robot artifact creature token.
//!
//! IMPLEMENTED (a faithful **partial** — its body is real, so it's not a husk):
//! - **Trample** (CR 702.19) — printed `Keyword`.
//! - **"Enters with +1/+1 counters equal to the mana spent to cast it"** — a `WouldEnterBattlefield(
//!   ItSelf)` replacement → `Rewrite::EntersWithCountersValue { PlusOnePlusOne, n: ValueExpr::ManaSpent }`
//!   (engine cap a2e2b13). `ManaSpent` is the total mana paid at cast (generic + colored + the chosen
//!   X), recorded on the object and reset on any zone change (CR 400.7), so Dyadrine cast for e.g.
//!   {3}{G}{W} (X=3) enters as a 5/6. This is its defining mechanic — with it, Dyadrine is a real
//!   scaling threat (printed 0/1 base + the counters), not a husk.
//!
//! INCOMPLETE — TRACKED (`fully_implemented: false`), one clause, NOT approximated:
//!   - **"Whenever you attack, you may remove a +1/+1 counter from each of two creatures you control.
//!     If you do, draw a card and create a 2/2 colorless Robot artifact creature token."** The
//!     `EventPattern::YouAttack` trigger is live (4613d51), and `Draw`/`CreateToken`/the Robot subtype
//!     all work — but the body needs two unbuilt capabilities: (i) **distinct two-target counter
//!     removal** (`Effect::PutCounters` resolves a single target; `ForEach` is uninterpreted), and
//!     (ii) the **reflexive "if you do"** gate (`Effect::Conditional` is uninterpreted). Authoring it
//!     now would mean un-enforced target distinctness + the draw/token firing even when you couldn't
//!     remove from two — a wrong approximation. Omitted entirely until those caps land. Flagged to engine.

use crate::basics::{CardType, Color, CounterKind};
use crate::cards::{mana_cost, CardDb, CardDef};
use crate::effects::ability::{Ability, ActionPattern, Keyword, Rewrite};
use crate::effects::target::CardFilter;
use crate::effects::value::ValueExpr;
use crate::state::Characteristics;
use crate::subtypes::{CreatureType, Supertype};

/// grp id (per-set ids live near their cards).
pub const DYADRINE_SYNTHESIS_AMALGAM: u32 = 116;

pub fn register(db: &mut CardDb) {
    // {X}{G}{W}: generic 0, one G + one W pip, one {X} symbol.
    let mut cost = mana_cost(0, &[(Color::Green, 1), (Color::White, 1)]);
    cost.x = 1;
    let chars = Characteristics {
        name: "Dyadrine, Synthesis Amalgam".to_string(),
        card_types: vec![CardType::Artifact, CardType::Creature],
        subtypes: vec![CreatureType::Construct.into()],
        supertypes: vec![Supertype::Legendary],
        colors: vec![Color::Green, Color::White],
        mana_cost: Some(cost),
        power: Some(0),
        toughness: Some(1),
        keywords: vec![Keyword::Trample],
        grp_id: DYADRINE_SYNTHESIS_AMALGAM,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![
            // "Dyadrine enters with a number of +1/+1 counters on it equal to the mana spent to cast it."
            Ability::Replacement {
                pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
                rewrite: Rewrite::EntersWithCountersValue {
                    kind: CounterKind::PlusOnePlusOne,
                    n: ValueExpr::ManaSpent,
                },
            },
        ],
        text: "Trample\nDyadrine enters with a number of +1/+1 counters on it equal to the amount of mana spent to cast it.\nWhenever you attack, you may remove a +1/+1 counter from each of two creatures you control. If you do, draw a card and create a 2/2 colorless Robot artifact creature token.".to_string(),
        // Tracked-incomplete: the "whenever you attack" ability needs distinct two-target counter
        // removal + a reflexive "if you do" — both unbuilt. The body (trample + mana-spent counters)
        // is faithful. See module docs.
        fully_implemented: false,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subtypes::Subtype;
    use expect_test::expect;

    #[test]
    fn dyadrine_synthesis_amalgam_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(DYADRINE_SYNTHESIS_AMALGAM).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Artifact, CardType::Creature]);
        assert_eq!(def.chars.subtypes, vec![Subtype::Creature(CreatureType::Construct)]);
        assert_eq!(def.chars.supertypes, vec![Supertype::Legendary]);
        assert_eq!(def.chars.colors, vec![Color::Green, Color::White]);
        assert_eq!(def.chars.keywords, vec![Keyword::Trample]); // trample works today
        assert_eq!(def.chars.mana_cost.as_ref().unwrap().x, 1); // {X} symbol present
        assert_eq!((def.chars.power, def.chars.toughness), (Some(0), Some(1))); // base; counters add
        // Tracked-incomplete: the "whenever you attack" ability is deferred (needs distinct
        // two-target removal + reflexive); the body (mana-spent counters) is faithful.
        assert!(!def.fully_implemented);
        // Only the enters-with-counters-=-mana-spent replacement; the attack ability is deliberately
        // absent (no silent approximation).
        expect![[r#"
            [
                Replacement {
                    pattern: WouldEnterBattlefield(
                        ItSelf,
                    ),
                    rewrite: EntersWithCountersValue {
                        kind: PlusOnePlusOne,
                        n: ManaSpent,
                    },
                },
            ]"#]]
        .assert_eq(&format!("{:#?}", def.abilities));
    }
}
