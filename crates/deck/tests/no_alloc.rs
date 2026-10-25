use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use std::sync::Arc;

use backspn_deck::{
    Command, CueKind, Deck, Grid, HOTCUES, HotcuePoint, KeylockRange, LoopEdge, Stems, Track,
};

struct Counting;

thread_local! {
    static ALLOCATIONS: Cell<usize> = const { Cell::new(0) };
}

#[allow(unsafe_code)]
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|c| c.set(c.get() + 1));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOCATIONS.with(|c| c.set(c.get() + 1));
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn allocations() -> usize {
    ALLOCATIONS.with(Cell::get)
}

#[test]
fn rendering_and_commands_do_not_allocate() {
    let rate = 44_100;
    let samples: Vec<f32> = (0..rate * 20)
        .flat_map(|i| [(i as f32 * 0.01).sin(), 0.5])
        .collect();
    let stem = || Track::from_stereo(rate as u32, samples.clone());
    let track = Arc::new(stem());
    let stems = Arc::new(Stems([stem(), stem(), stem(), stem()]));
    let mut deck = Deck::new(rate as u32);
    let mut stem_deck = Deck::with_stems(rate as u32);
    let mut out = vec![0.0f32; 2 * 512];
    let mut layers = [[0.0f32; 2 * 512]; 4];
    deck.handle(Command::Load(Arc::clone(&track)));
    stem_deck.handle(Command::LoadStems(Arc::clone(&stems)));

    let commands = [
        Command::SetGrid(Some(Grid {
            bpm: 128.0,
            anchor_s: 0.05,
            ..Grid::FIXED
        })),
        Command::SetSnap(true),
        Command::SetQuantize(true),
        Command::Play,
        Command::SetTempo(1.08),
        Command::Hotcue(0),
        Command::AutoLoop { beats: 4.0 },
        Command::LoopScale(0.5),
        Command::BeatJump {
            beats: 8.0,
            stay_in_loop: false,
        },
        Command::MoveLoop {
            edge: LoopEdge::Out,
            beats: None,
            steps: -1,
        },
        Command::SetKeylock(true),
        Command::SetKeylockRange(KeylockRange::Normal),
        Command::SetKeyAdjust(-2.0),
        Command::Hotcue(0),
        Command::LoopToggle,
        Command::SetHotcues([None; HOTCUES]),
        Command::JumpToCue(HotcuePoint {
            start_s: 3.0,
            kind: CueKind::Loop,
            loop_s: Some(1.0),
        }),
        Command::Seek { seconds: 10.0 },
        Command::CueDown,
        Command::CueUp,
        Command::CupDown,
        Command::CupUp,
        Command::SetKeylock(false),
        Command::Pause,
    ];
    for command in commands.clone() {
        deck.handle(command.clone());
        deck.process(&mut out);
        stem_deck.handle(command);
        let [a, b, c, d] = &mut layers;
        stem_deck.process_layers(&mut [a, b, c, d]);
    }

    let before = allocations();
    for command in commands {
        deck.handle(command.clone());
        stem_deck.handle(command);
        for _ in 0..20 {
            deck.process(&mut out);
            let [a, b, c, d] = &mut layers;
            stem_deck.process_layers(&mut [a, b, c, d]);
            stem_deck.process(&mut out);
        }
        let _ = deck.state();
    }
    assert_eq!(
        allocations() - before,
        0,
        "the deck allocated on the audio path"
    );
}
