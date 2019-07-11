#![feature(core_intrinsics)]
extern crate exvm;

use exvm::gc::copying::CopyGC;
use exvm::heap::*;
use exvm::zalloc::*;

fn main() {
    init_zonealloc();

    let mut gc = CopyGC::new();
    for _ in 0..100000 {
        gc.alloc_tagged(HeapTag::Number, 128);
    }
    gc.collect_garbage();
}
