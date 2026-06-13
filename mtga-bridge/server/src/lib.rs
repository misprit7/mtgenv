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
//! Module layout:
//!   - Dependency-free protocol core: [`frame`], [`envelope`], [`cmds`].
//!   - I/O layer (tokio + rustls + flate2): [`cert`], [`jwt`], [`http_stub`],
//!     [`frontdoor`]. These speak TLS and drive the live login + FrontDoor flow.

// --- dependency-free protocol core ---
pub mod cmds;
pub mod envelope;
pub mod frame;

// --- I/O layer (async/TLS) ---
pub mod cert;
pub mod frontdoor;
pub mod http_stub;
pub mod jwt;
pub mod logging;
