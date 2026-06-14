//! Misc / starter cards — the basic lands that have no meaningful first-printing set. Every
//! non-basic prototype card now lives in a `<setcode>/` folder keyed by its first-printing set
//! (see the card-push spec); only the basics remain here.
//!
//! [`register`] inserts the basics. The card *builders* (`creature`/`spell`/`aura`/…) and id
//! constants live in the parent (`crate::cards`).

use crate::cards::CardDb;

pub mod basics;

/// Insert every misc/starter card into `db`.
pub fn register(db: &mut CardDb) {
    basics::register(db);
}
