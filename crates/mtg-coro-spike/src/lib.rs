//! M3.0 spike (throwaway) — proves the stackful-coroutine substrate for the resumable engine
//! (docs/design/RESUMABLE_ENGINE.md, Option B). Nothing here is production code; it exists only to
//! answer the lead's go/no-go questions and is deleted once M3.0 is signed off.
//!
//! Proven here (see the tests):
//!  1. corosensei builds offline and its yield→resume(input)→return API matches the design.
//!  2. the REAL engine runs to completion inside a fiber (stack switching is transparent to it).
//!  3. worst-case fiber STACK SIZE — measured by painting the fiber stack and scanning the
//!     high-water mark across every preset deck × many seeds.
//!  4. SEND — corosensei::Coroutine is `!Send` by design, but the crate docs sanction a manual
//!     `unsafe impl Send` when all stack data is Send (our case: EngineCore is Send, agents live
//!     in the driver). A live *suspended* fiber is moved across a thread boundary and resumed.
//!  5. PANIC ISOLATION — a panicking fiber is contained by `catch_unwind` at the resume seam; the
//!     rest of the fleet finishes. (Matters: sos-cards' debug_assert guard panics on unwired leaves.)

#![cfg(test)]

use corosensei::{Coroutine, CoroutineResult};
use mtg_core::agent::RandomAgent;
use mtg_core::cards::{self, preset_deck};
use mtg_core::priority::{Engine, Outcome};

/// The preset decks a fleet would actually run (the sizing/behaviour population).
const DECKS: &[&str] = &["heralds", "burn", "bears", "demo", "selesnya"];

/// Run one full random self-play game (both seats `RandomAgent`) to completion and return its
/// outcome. `RandomAgent` answers synchronously, so `run_game` never suspends — perfect for
/// exercising the engine's real call depth inside a fiber without needing the M3.2 yield wiring.
fn run_random_game(deck: &[u32], seed: u64) -> Outcome {
    let state = cards::build_game(seed, &[deck, deck]);
    let mut e = Engine::new(
        state,
        vec![Box::new(RandomAgent::new(seed)), Box::new(RandomAgent::new(seed ^ 0x9E37))],
    );
    e.run_game();
    e.outcome()
}

// ── 1 & round-trip: the yield/resume/return shape ───────────────────────────────────────────

/// yield a value, resume WITH an input, yield again, then return — the exact shape `ask` needs
/// (suspend a DecisionRequest, resume with a DecisionResponse).
#[test]
fn round_trips_yield_resume_return() {
    let mut coro = Coroutine::new(|yielder, first: i32| {
        let a = yielder.suspend(first + 1); // yield first+1, get next resume input
        let b = yielder.suspend(a * 2); // yield a*2, get next resume input
        format!("done: first={first} a={a} b={b}")
    });
    assert!(matches!(coro.resume(10), CoroutineResult::Yield(11)));
    assert!(matches!(coro.resume(5), CoroutineResult::Yield(10)));
    match coro.resume(7) {
        CoroutineResult::Return(r) => assert_eq!(r, "done: first=10 a=5 b=7"),
        CoroutineResult::Yield(_) => panic!("expected the return"),
    }
}

// ── 2: the real engine runs inside a fiber ──────────────────────────────────────────────────

#[test]
fn real_engine_game_runs_to_completion_in_a_fiber() {
    let deck = preset_deck("heralds").unwrap();
    let mut coro = Coroutine::<(), (), Outcome>::new(move |_y, ()| run_random_game(&deck, 1));
    match coro.resume(()) {
        CoroutineResult::Return(outcome) => {
            // A real game finished on the fiber's stack; the engine is oblivious to being a fiber.
            assert!(outcome.turns > 0, "the game ran to completion inside the fiber");
        }
        CoroutineResult::Yield(()) => unreachable!("RandomAgent never suspends"),
    }
}

// ── 3: worst-case fiber stack size (painted-stack high-water) ────────────────────────────────

/// Run `run_random_game` inside a fiber whose stack we own and pre-paint with a sentinel, then
/// scan how far down from the top the game wrote → exact stack high-water for that game.
/// Returns (outcome, bytes_used). Unix-only (uses `DefaultStack`'s mmap layout).
#[cfg(unix)]
fn stack_high_water(deck: &[u32], seed: u64) -> (Outcome, usize) {
    use corosensei::stack::{DefaultStack, Stack};

    const STACK_SIZE: usize = 4 << 20; // 4 MiB fiber — huge headroom so the paint region is safe
    const PAINT: usize = 2 << 20; // paint the top 2 MiB below `base` (well clear of the guard page)
    const SENTINEL: u8 = 0xAB;

    let stack = DefaultStack::new(STACK_SIZE).expect("alloc fiber stack");
    let base = stack.base().get(); // high address; stack grows DOWN from here
    let paint_lo = base - PAINT;
    // SAFETY: [paint_lo, base) is inside the writable region (2 MiB below the 4 MiB top, far above
    // the guard page); the fiber hasn't run yet so nothing here is live.
    unsafe { std::ptr::write_bytes(paint_lo as *mut u8, SENTINEL, PAINT) };

    let deck_v = deck.to_vec();
    let mut coro = Coroutine::<(), (), Outcome, _>::with_stack(
        stack,
        move |_y, ()| run_random_game(&deck_v, seed),
    );
    let outcome = match coro.resume(()) {
        CoroutineResult::Return(o) => o,
        CoroutineResult::Yield(()) => unreachable!(),
    };
    // Scan up from the painted bottom for the first byte the game overwrote → deepest stack point.
    // SAFETY: the coroutine (hence its stack mmap) is still alive; we read only the painted region.
    let mut lo = paint_lo;
    unsafe {
        while lo < base && *(lo as *const u8) == SENTINEL {
            lo += 1;
        }
    }
    let used = base - lo;
    drop(coro);
    (outcome, used)
}

#[cfg(unix)]
#[test]
fn engine_stack_high_water_across_decks() {
    let mut worst = 0usize;
    let mut worst_where = ("", 0u64);
    for &name in DECKS {
        let deck = preset_deck(name).unwrap();
        for seed in 0..25u64 {
            let (outcome, used) = stack_high_water(&deck, seed);
            assert!(outcome.turns > 0);
            if used > worst {
                worst = used;
                worst_where = (name, seed);
            }
        }
    }
    // Report (visible with `--nocapture`). This is the number the lead asked for.
    eprintln!(
        "[stack] worst-case fiber stack high-water: {worst} bytes (~{} KiB) at deck={} seed={} \
         over {} games; recommended fiber stack = {} KiB (8x headroom, min 256 KiB)",
        worst.div_ceil(1024),
        worst_where.0,
        worst_where.1,
        DECKS.len() * 25,
        (worst * 8).div_ceil(1024).max(256),
    );
    // Sanity bound: the engine has no unbounded recursion, so a random game must stay well under
    // 1 MiB. If this ever trips, the recursion assumption broke and sizing must be revisited.
    assert!(worst < (1 << 20), "unexpectedly deep engine stack: {worst} bytes");
    assert!(worst > 0, "measurement failed to observe any stack use");
}

// ── 4: Send — move a live suspended fiber across threads ─────────────────────────────────────

/// corosensei marks `Coroutine` `!Send` conservatively (it can't prove the stack is Send). Per the
/// crate docs, a manual `unsafe impl Send` is sound when all stack data is Send. In the real design
/// the fiber's stack holds only `EngineCore` + engine locals (all Send; agents live in the driver),
/// so this is exactly that sanctioned pattern.
struct SendFiber<I, Y, R>(Coroutine<I, Y, R>);
// SAFETY: spike only carries `i32` on the stack (Send). In production the invariant is "EngineCore
// and everything it holds is Send" — enforced by keeping non-Send agents out of the core.
unsafe impl<I, Y, R> Send for SendFiber<I, Y, R> {}

#[test]
fn suspended_fiber_moves_across_a_thread_boundary() {
    let mut coro = Coroutine::new(|y, first: i32| {
        let a = y.suspend(first + 1); // suspend on the ORIGINAL thread
        a * 10 // ...resume + return on ANOTHER thread
    });
    // Run to the first suspend on this thread.
    assert!(matches!(coro.resume(1), CoroutineResult::Yield(2)));
    // Move the LIVE, suspended fiber to a worker thread and finish it there.
    let fiber = SendFiber(coro);
    let out = std::thread::spawn(move || {
        let mut f = fiber;
        match f.0.resume(5) {
            CoroutineResult::Return(r) => r,
            CoroutineResult::Yield(_) => unreachable!(),
        }
    })
    .join()
    .expect("worker thread finished the fiber");
    assert_eq!(out, 50, "the fiber resumed correctly after crossing threads");
}

#[test]
fn real_engine_fiber_runs_on_a_worker_thread() {
    // Build the game-fiber on this thread, hand it (Send-wrapped) to a worker, run to completion.
    let deck = preset_deck("bears").unwrap();
    let coro = Coroutine::<(), (), Outcome>::new(move |_y, ()| run_random_game(&deck, 7));
    let fiber = SendFiber(coro);
    let outcome = std::thread::spawn(move || {
        let mut f = fiber;
        match f.0.resume(()) {
            CoroutineResult::Return(o) => o,
            CoroutineResult::Yield(()) => unreachable!(),
        }
    })
    .join()
    .expect("worker finished the game");
    assert!(outcome.turns > 0);
}

// ── 5: panic isolation ──────────────────────────────────────────────────────────────────────

#[test]
fn panicking_fiber_is_isolated_from_the_fleet() {
    // Silence the panic hook so the deliberate panic doesn't spam the test log.
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    // A "fleet" of 5 fibers; fiber #2 panics in its body (stands in for sos-cards' debug_assert on
    // an unwired card leaf). Each resume is wrapped in catch_unwind at the boundary.
    let results: Vec<Result<i32, ()>> = (0..5i32)
        .map(|i| {
            let mut coro = Coroutine::<(), (), i32>::new(move |_y, ()| {
                if i == 2 {
                    panic!("simulated unwired-leaf debug_assert in fiber {i}");
                }
                i * 10
            });
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| match coro.resume(()) {
                CoroutineResult::Return(r) => r,
                CoroutineResult::Yield(()) => unreachable!(),
            }))
            .map_err(|_| ())
        })
        .collect();

    std::panic::set_hook(prev);

    // The panic was contained to its own fiber; every other game finished normally.
    assert_eq!(results, vec![Ok(0), Ok(10), Err(()), Ok(30), Ok(40)]);
}
