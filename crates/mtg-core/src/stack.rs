//! The stack: putting spells/abilities on it, responding, and resolving
//! (CR 405, 608) — including illegal-target-on-resolution handling (CR 608.2b).
//!
//! Milestone 2 implements the **data structure + LIFO discipline**. Resolution that
//! *runs effects* arrives with the effect runtime (M4); the structural resolution
//! (a permanent spell enters the battlefield, an instant/sorcery goes to its owner's
//! graveyard, an ability ceases to exist — CR 608.2n / 608.3) is driven from the engine
//! in `priority.rs`.

use serde::{Deserialize, Serialize};

use crate::basics::Target;
use crate::effects::action::Action;
use crate::ids::{ObjId, PlayerId, StackId};

/// What a stack object *is* (CR 113.1c, 112.1): a spell (a card/copy on the stack) or an
/// activated/triggered ability (a non-card object with only its source ability's text).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StackObjectKind {
    /// A spell — references the card/copy object now on the stack.
    Spell(ObjId),
    /// An activated or triggered ability on the stack. `index` selects which ability of the
    /// source object (`StackObject::source`) it is — into that object's `CardDef.abilities`,
    /// looked up by `grp_id` (which persists across zones, so a dies-trigger still resolves).
    Ability { index: u32 },
    /// A delayed triggered ability (CR 603.7) that fired — it has no printed `CardDef` ability to
    /// index, so it carries its own concrete [`Action`]s (e.g. Earthbend's "return it tapped").
    DelayedAbility { actions: Vec<Action> },
}

/// One object on the stack (CR 405.1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StackObject {
    pub id: StackId,
    /// The player who cast the spell / controls the ability (CR 112.2 / 113.8).
    pub controller: PlayerId,
    /// The source object the spell/ability comes from (for abilities; `None` for a copy).
    pub source: Option<ObjId>,
    pub kind: StackObjectKind,
    /// Targets chosen at put-on-stack time (CR 601.2c); rechecked on resolution (608.2b).
    pub targets: Vec<Target>,
    /// The value chosen for X at cast/activation (CR 601.2b) when the cost had `{X}`, carried to
    /// resolution so the effect's `ValueExpr::X` reads it. `None` when there is no X.
    pub x: Option<u32>,
    /// The mode indices chosen for a modal spell/ability at cast/activation (CR 700.2 / 601.2b),
    /// carried to resolution so it runs only those modes (targets were collected only for them).
    /// Empty for non-modal objects.
    pub modes: Vec<u32>,
}

/// The stack (CR 405). LIFO: the top is the **last** element; new objects push on top, and
/// resolution takes the top (CR 405.2/405.5).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stack {
    pub items: Vec<StackObject>,
}

impl Stack {
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn len(&self) -> usize {
        self.items.len()
    }
    /// Put a new object on top of the stack (CR 405.2).
    pub fn push(&mut self, obj: StackObject) {
        self.items.push(obj);
    }
    /// The top object (next to resolve), if any.
    pub fn top(&self) -> Option<&StackObject> {
        self.items.last()
    }
    /// Remove and return the top object for resolution (CR 405.5).
    pub fn pop(&mut self) -> Option<StackObject> {
        self.items.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stack_is_lifo() {
        let mut s = Stack::default();
        assert!(s.is_empty());
        s.push(StackObject {
            id: StackId(1),
            controller: PlayerId(0),
            source: None,
            kind: StackObjectKind::Ability { index: 0 },
            targets: vec![],
            x: None,
            modes: Vec::new(),
        });
        s.push(StackObject {
            id: StackId(2),
            controller: PlayerId(1),
            source: None,
            kind: StackObjectKind::Ability { index: 0 },
            targets: vec![],
            x: None,
            modes: Vec::new(),
        });
        assert_eq!(s.len(), 2);
        // Last-in (id 2) resolves first.
        assert_eq!(s.top().unwrap().id, StackId(2));
        assert_eq!(s.pop().unwrap().id, StackId(2));
        assert_eq!(s.pop().unwrap().id, StackId(1));
        assert!(s.is_empty());
    }
}
