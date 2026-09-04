//! Guards the heap contract of each execution path.
//!
//! `step`/`run_for_cycles` must never allocate, so bare-metal hosts can embed
//! the emulator without a real heap. `run_batch` is allowed to allocate: it
//! builds the 64K decode table and the trace caches on first use.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::Cell;
use m68k::{CpuCore, CpuType, LinearMemoryBus, StepResult};

// Per-thread so that tests running in parallel cannot pollute each other's
// counts. `const`-initialized to keep the allocator itself allocation-free.
thread_local! {
    static LIVE: Cell<usize> = const { Cell::new(0) };
    static PEAK: Cell<usize> = const { Cell::new(0) };
}

struct Counting;

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // `try_with` because TLS is unavailable during thread teardown.
        let _ = LIVE.try_with(|live| {
            let now = live.get() + layout.size();
            live.set(now);
            PEAK.with(|peak| peak.set(peak.get().max(now)));
        });
        unsafe { std::alloc::System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let _ = LIVE.try_with(|live| live.set(live.get().saturating_sub(layout.size())));
        unsafe { std::alloc::System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

/// Records the peak heap growth on this thread while `body` runs.
fn peak_growth(body: impl FnOnce()) -> usize {
    let base = LIVE.with(Cell::get);
    PEAK.with(|peak| peak.set(base));
    body();
    PEAK.with(Cell::get).saturating_sub(base)
}

/// A tight backward-branch loop: exactly the shape the trace recorder wants.
fn loop_program() -> LinearMemoryBus {
    let mut mem = LinearMemoryBus::new(1024 * 1024);
    mem.write_long_at(0, 0x0008_0000); // reset SSP
    mem.write_long_at(4, 0x0000_1000); // reset PC
    mem.write_word_at(0x1000, 0x7000); // moveq #0,d0
    mem.write_word_at(0x1002, 0x323C); // move.w #$0FFF,d1
    mem.write_word_at(0x1004, 0x0FFF);
    mem.write_word_at(0x1006, 0x5280); // addq.l #1,d0
    mem.write_word_at(0x1008, 0x51C9); // dbra d1,-4
    mem.write_word_at(0x100A, 0xFFFC);
    mem.write_word_at(0x100C, 0x4E71); // nop
    mem.write_word_at(0x100E, 0x60FE); // bra.s *
    mem
}

fn booted(mem: &mut LinearMemoryBus) -> CpuCore {
    let mut cpu = CpuCore::new();
    cpu.set_cpu_type(CpuType::M68000);
    cpu.reset(mem);
    cpu
}

#[test]
fn interpreter_path_never_allocates() {
    let mut mem = loop_program();
    let mut cpu = booted(&mut mem);

    let grew = peak_growth(|| {
        for _ in 0..200_000 {
            if !matches!(cpu.step(&mut mem), StepResult::Ok { .. }) {
                break;
            }
        }
    });

    assert_eq!(
        grew, 0,
        "step() allocated {grew} bytes; the no_std interpreter path must stay heap-free"
    );
}

#[test]
fn run_for_cycles_path_never_allocates() {
    let mut mem = loop_program();
    let mut cpu = booted(&mut mem);

    let grew = peak_growth(|| {
        for _ in 0..200 {
            cpu.run_for_cycles(&mut mem, 1_000);
        }
    });

    assert_eq!(
        grew, 0,
        "run_for_cycles() allocated {grew} bytes; the no_std interpreter path must stay heap-free"
    );
}

#[test]
fn batch_path_builds_its_caches_on_the_heap() {
    let mut mem = loop_program();
    let mut cpu = booted(&mut mem);

    let grew = peak_growth(|| {
        for _ in 0..40 {
            cpu.run_batch(&mut mem, 10_000, &[]);
        }
    });

    // Documents why `run_batch` is the path a memory-constrained host avoids:
    // a 1 MiB decode table plus multi-megabyte trace caches.
    assert!(
        grew > 1024 * 1024,
        "expected run_batch() to build its caches, saw only {grew} bytes"
    );
}
