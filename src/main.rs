#![feature(core_intrinsics)]
extern crate exvm;

use exvm::gc::copying::{formatted_size, CopyGC};
use exvm::heap::*;
use exvm::zalloc::*;

fn main() {
    let mut gc = CopyGC::new();
    let mut mbs = 0;
    let my_number = gc.alloc_tagged(HeapTag::Number, 8);
    gc.collect_garbage();

    unsafe {
        *my_number.to_mut_ptr::<i64>() = 42;
        println!("{}", *my_number.to_ptr::<i64>());
        println!("{:?}", my_number.to_ptr::<u8>().offset(1));
    }

    println!("Total allocated: {}", formatted_size(mbs));
}
