use super::*;
use crate::heap::*;
use crate::mem;
use crate::os;
use crate::os::ProtType;
pub struct CopyGC {
    total: Region,
    separator: Address,
    alloc: alloc::BumpAllocator,
    grey: Vec<*mut HValue>,
    black: Vec<*mut HValue>,
}

struct FormattedSize {
    size: usize,
}

impl fmt::Display for FormattedSize {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let ksize = (self.size as f64) / 1024f64;

        if ksize < 1f64 {
            return write!(f, "{}B", self.size);
        }

        let msize = ksize / 1024f64;

        if msize < 1f64 {
            return write!(f, "{:.1}K", ksize);
        }

        let gsize = msize / 1024f64;

        if gsize < 1f64 {
            write!(f, "{:.1}M", msize)
        } else {
            write!(f, "{:.1}G", gsize)
        }
    }
}

fn formatted_size(size: usize) -> FormattedSize {
    FormattedSize { size }
}

extern "C" {
    fn malloc(size: usize) -> *mut u8;
}

impl CopyGC {
    pub fn new() -> CopyGC {
        let alignment = 2 * os::page_size() as usize;
        let heap_size = mem::align_usize(HEAP_SIZE, alignment);
        //let ptr = os::mmap(heap_size, os::ProtType::Writable);
        let ptr = unsafe { malloc(heap_size) };
        if ptr.is_null() {
            panic!("could not allocate semi space of size {} bytes", heap_size);
        }

        let heap_start = Address::from_ptr(ptr);
        let heap = heap_start.offset(1).region_start(heap_size);

        let semi_size = heap_size / 2;
        let separator = heap_start.offset(1).offset(semi_size);

        CopyGC {
            total: heap,
            separator,
            alloc: alloc::BumpAllocator::new(heap_start, separator),
            grey: Vec::new(),
            black: Vec::new(),
        }
    }

    pub fn process_grey(&mut self, top: &mut Address, from_space: Region) {
        while !self.grey.is_empty() {
            let item: *mut HValue = self.grey.remove(0);
            if HValue::is_unboxed(item as *mut _) || item == HeapTag::Nil as u8 as *mut HValue {
                continue;
            }
            unsafe {
                if !(*item).is_gc_marked() {
                    if !from_space.contains(Address::from_ptr(item)) {
                        if !(*item).is_soft_gc_marked() {
                            (*item).set_soft_gc_mark();
                            self.black.push(item);
                            self.visit(item);
                        }
                        continue;
                    }
                    let addr = self.copy(Address::from_ptr(item), top, from_space);
                    if !(*item).is_gc_marked() {
                        (*item).set_gc_mark(addr.to_mut_ptr());
                    }
                    self.visit(item);
                } else {
                    if !(*item).is_gc_marked() {
                        (*item).set_gc_mark((*item).get_gc_mark());
                    }
                }
            }
        }
    }

    pub fn alloc_tagged(&mut self, tag: HeapTag, size: usize) -> Address {
        let ptr = self.alloc.bump_alloc(size + 8).to_mut_ptr::<u8>();

        if !ptr.is_null() {
            unsafe {
                *((ptr as isize + HValue::TAG_OFFSET) as *mut u8) = tag as u8;
            }
            return Address::from_ptr(ptr);
        }

        println!("alloc_tagged: Collecting garbage");
        self.collect_garbage();
        let ptr = self.alloc.bump_alloc(size + 8).to_mut_ptr::<u8>();
        unsafe {
            *((ptr as isize + HValue::TAG_OFFSET) as *mut u8) = tag as u8;
        }
        Address::from_ptr(ptr)
    }

    pub fn alloc(&mut self, size: usize) -> Address {
        let ptr = self.alloc.bump_alloc(size);

        if ptr.is_non_null() {
            return ptr;
        }
        self.collect_garbage();
        self.alloc.bump_alloc(size)
    }

    pub fn collect_garbage(&mut self) {
        let start_time = time::PreciseTime::now();

        let to_space = self.to_space();
        let from_space = self.from_space();

        // determine size of heap before collection
        let old_size = self.alloc.top().offset_from(from_space.start);

        let mut top = to_space.start;
        let mut scan = top;

        /*
            for root in rootset {


            }
        */

        while scan < top {
            let size = unsafe { (*scan.to_mut_ptr::<HValue>()).size() };
            self.grey.push(scan.to_mut_ptr::<HValue>());
            scan = scan.offset(size);
        }

        self.process_grey(&mut top, from_space);

        while self.black.len() != 0 {
            let value = self.black.remove(0);
            unsafe {
                (*value).reset_soft_gc_mark();
            }
        }
        /*        if cfg!(debug_assertions) {
                    os::mprotect(from_space.start.to_ptr(), from_space.size(), ProtType::None);
                }
        */
        self.alloc.reset(top, to_space.end);

        let new_size = top.offset_from(to_space.start);

        let garbage = old_size - new_size;
        let garbage_ratio = if old_size == 0 {
            0f64
        } else {
            (garbage as f64 / old_size as f64) * 100f64
        };
        let end = time::PreciseTime::now();
        println!(
            "Copy GC: {:.1} ms, {}->{} size, {}/{:.0}% garbage",
            start_time.to(end).num_milliseconds(),
            formatted_size(old_size),
            formatted_size(new_size),
            formatted_size(garbage),
            garbage_ratio,
        );
    }

    pub fn from_space(&self) -> Region {
        if self.alloc.limit() == self.separator {
            Region::new(self.total.start, self.separator)
        } else {
            Region::new(self.separator, self.total.end)
        }
    }

    pub fn to_space(&self) -> Region {
        if self.alloc.limit() == self.separator {
            Region::new(self.separator, self.total.end)
        } else {
            Region::new(self.total.start, self.separator)
        }
    }

    pub fn copy(&self, from: Address, top: &mut Address, from_space: Region) -> Address {
        let addr = *top;
        let hval: &HValue = unsafe { &(*HValue::cast(from.to_mut_ptr())) };

        if hval.is_gc_marked() {
            return Address::from_ptr(hval.get_gc_mark());
        }
        let (_, size) = unsafe { (*HValue::cast(from.to_mut_ptr())).copy_to(top) };
        *top = top.offset(size);

        hval.set_gc_mark(addr.to_mut_ptr());

        addr
    }

    pub fn visit(&mut self, value: *mut HValue) {
        match unsafe { (*value).tag() } {
            HeapTag::Context => self.visit_ctx(value as *mut _),
            HeapTag::Function => self.visit_function(value as *mut _),
            HeapTag::Object => self.visit_obj(value as *mut _),
            HeapTag::Array => self.visit_obj(value as *mut _),
            HeapTag::Map => self.visit_obj(value as *mut _),
            HeapTag::String => {
                // TODO
                return;
            }

            _ => {
                unsafe {
                    println!("Skip {:?}", (*value).tag());
                }
                return;
            }
        }
    }

    pub fn visit_ctx(&mut self, ctx: *mut HContext) {
        unsafe {
            let ctx: &HContext = &*ctx;

            if ctx.has_parent() {
                self.grey.push(HValue::cast(ctx.parent()));
            }

            for i in 0..ctx.slots() {
                if !ctx.has_slot(i) {
                    continue;
                }
                self.grey.push(ctx.get_slot(i));
            }
        }
    }

    pub fn visit_obj(&mut self, obj: *mut HObject) {
        unsafe {
            let obj: &HObject = &*obj;
            if obj.proto() != std::ptr::null_mut() {
                self.grey.push(HValue::cast(obj.proto())); // TODO: This may fail, add weak references
            }
            self.grey.push(HValue::cast(obj.map()));
        }
    }

    pub fn visit_function(&mut self, fun: *mut HFunction) {
        unsafe {
            let fun: &HFunction = &*fun;
            if fun.parent_slot() != std::ptr::null_mut()
                && fun.parent() != BINDING_CONTEXT_TAG as *mut u8
            {
                self.grey.push(HValue::cast(fun.parent()));
            }

            if fun.root_slot() != std::ptr::null_mut() {
                self.grey.push(HValue::cast(fun.root()));
            }
        }
    }

    pub fn visit_array(&mut self, array: *mut HArray) {
        unsafe {
            // Array is object,so we need to cast it to object to get object table
            let array: &HObject = &*(array as *mut HObject);
            self.grey.push(HValue::cast(array.map()));
        }
    }

    pub fn visit_map(&mut self, map: *mut HMap) {
        unsafe {
            let map: &HMap = &*map;
            let size = map.size() << 1;
            for i in 0..size {
                if !map.has_slot(i) {
                    continue;
                }
                self.grey.push(map.get_slot(i));
            }
        }
    }
}
