//! mtga-bridge: a stub backend the real MTG Arena client can connect to.
//!
//! Goal: let the actual MTGA client act as a front-end for our own backend
//! (eventually the mtgenv rules engine), as an alternative to the project's web
//! UI. This crate is deliberately self-contained and NOT wired to the engine yet.
//!
//! Milestones:
//!   M0  redirect + TLS  — client trusts our cert and connects (hosts script + TLS listener)
//!   M1  login           — stub PlayFab/platform HTTPS so the client logs in
//!   M2  home screen      — answer the FrontDoor startup commands so home renders   <-- current target
//!   M3  start a match    — accept EventAiBotMatch, hand the client our GRE endpoint
//!   M4  gameplay         — speak the GRE protocol (later: bridge to mtgenv)
//!
//! What's implemented today: the wire foundation common to every channel — the
//! [`frame`] codec and the FrontDoor [`envelope`] decode. The TLS listener, the
//! HTTPS login stub, and the FrontDoor command responses are added next, once the
//! login + startup-command surfaces (currently under reverse-engineering) land.

pub mod cmds;
pub mod envelope;
pub mod frame;
