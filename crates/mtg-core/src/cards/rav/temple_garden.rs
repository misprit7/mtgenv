//! Temple Garden — Land — Forest Plains (first printed RAV, Ravnica: City of Guilds). A Selesnya
//! shock land.
//!
//! Oracle: "({T}: Add {G} or {W}.) As this land enters, you may pay 2 life. If you don't, it
//! enters tapped."
//!
//! Fully implemented (no deferrals):
//! - {G}/{W} mana is **intrinsic** — the `Forest`+`Plains` land subtypes (CR 305.6), so there's no
//!   mana ability on the card at all; the engine grants `{T}: Add {G}`/`{T}: Add {W}` from the types.
//! - The shock clause is a `WouldEnterBattlefield(ItSelf)` replacement → `EntersTappedUnlessPay{2}`
//!   (C11): the engine asks the controller to pay 2 life as it enters — pay → untapped, decline → tapped.

use crate::basics::CardType;
use crate::cards::{CardDb, CardDef};
use crate::effects::ability::{Ability, ActionPattern, Rewrite};
use crate::effects::target::CardFilter;
use crate::state::Characteristics;
use crate::subtypes::LandType;

/// grp id (per-set ids live near their cards).
pub const TEMPLE_GARDEN: u32 = 109;

pub fn register(db: &mut CardDb) {
    let chars = Characteristics {
        name: "Temple Garden".to_string(),
        card_types: vec![CardType::Land],
        subtypes: vec![LandType::Forest.into(), LandType::Plains.into()],
        grp_id: TEMPLE_GARDEN,
        ..Default::default()
    };
    db.insert(CardDef {
        chars,
        abilities: vec![Ability::Replacement {
            pattern: ActionPattern::WouldEnterBattlefield(CardFilter::ItSelf),
            rewrite: Rewrite::EntersTappedUnlessPay { life: 2 },
        }],
        text: "({T}: Add {G} or {W}.)\nAs this land enters, you may pay 2 life. If you don't, it enters tapped.".to_string(),
        fully_implemented: true,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subtypes::Subtype;
    use expect_test::expect;

    #[test]
    fn temple_garden_ir() {
        let mut db = CardDb::default();
        register(&mut db);
        let def = db.get(TEMPLE_GARDEN).unwrap();
        assert_eq!(def.chars.card_types, vec![CardType::Land]);
        // Forest + Plains subtypes → intrinsic G/W mana (no mana ability).
        assert_eq!(
            def.chars.subtypes,
            vec![Subtype::from(LandType::Forest), Subtype::from(LandType::Plains)]
        );
        assert!(!def.is_mana_source()); // mana is intrinsic from the types, not an IR ability
        assert!(def.fully_implemented);
        expect![[r#"
            [
                Replacement {
                    pattern: WouldEnterBattlefield(
                        ItSelf,
                    ),
                    rewrite: EntersTappedUnlessPay {
                        life: 2,
                    },
                },
            ]"#]].assert_eq(&format!("{:#?}", def.abilities));
    }

    /// #60 end-to-end (the REAL play-land path): playing Temple Garden fires its
    /// `EntersTappedUnlessPay{2}` shock replacement, which asks the controller to pay 2 life **as it
    /// enters**. Pay → lose 2 life, enters **untapped**; decline → keep the life, enters **tapped**.
    /// This whole clause had no behaviour coverage; here both branches are driven through `play_land`.
    #[test]
    fn temple_garden_pays_two_life_or_enters_tapped() {
        use crate::agent::{Agent, DecisionRequest, DecisionResponse, PlayerView};
        use crate::basics::Zone;
        use crate::cards::starter_db;
        use crate::ids::PlayerId;
        use crate::priority::Engine;
        use crate::state::GameState;
        use std::sync::Arc;

        // Answers the shock land's "pay 2 life?" Confirm with a fixed choice.
        #[derive(Clone)]
        struct ShockAgent {
            pay: bool,
        }
        impl Agent for ShockAgent {
            fn decide(&mut self, _v: &PlayerView, req: &DecisionRequest) -> DecisionResponse {
                match req {
                    DecisionRequest::Confirm { .. } => DecisionResponse::Bool(self.pay),
                    _ => DecisionResponse::Pass,
                }
            }
        }

        // Returns (life_after, entered_tapped) for a given pay/decline choice.
        let run = |pay: bool| -> (i32, bool) {
            let mut state = GameState::new(2, 1);
            state.set_card_db(Arc::new(starter_db()));
            let temple = {
                let c = state.card_db().get(TEMPLE_GARDEN).unwrap().chars.clone();
                state.add_card(PlayerId(0), c, Zone::Hand)
            };
            let mut e = Engine::new(
                state,
                vec![Box::new(ShockAgent { pay }), Box::new(ShockAgent { pay })],
            );
            let life_before = e.state.player(PlayerId(0)).life;
            e.play_land(PlayerId(0), temple); // shock replacement asks "pay 2 life?" during commit
            let tapped = e.state.object(temple).status.tapped;
            assert_eq!(e.state.object(temple).zone, Zone::Battlefield, "the land entered play");
            (life_before - e.state.player(PlayerId(0)).life, tapped)
        };

        // Pay 2 life → lost 2, entered untapped.
        assert_eq!(run(true), (2, false), "paid 2 life → untapped");
        // Decline → lost 0, entered tapped.
        assert_eq!(run(false), (0, true), "declined → enters tapped");
    }
}
