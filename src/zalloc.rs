extern "C" {
    fn malloc(size: usize) -> *mut u8;
    fn free(ptr: *mut u8);
}

#[derive(Clone, PartialEq, PartialOrd, Ord, Eq, Hash, Debug)]
pub struct ZoneBlock {
    data: *mut u8,
    offset: usize,
    size: usize,
}

impl ZoneBlock {
    pub fn new(size: usize) -> ZoneBlock {
        ZoneBlock {
            data: {
                let ptr = unsafe { malloc(size) };
                if ptr.is_null() {
                    std::process::exit(1);
                }
                ptr
            },
            offset: 0,
            size: 0,
        }
    }

    #[inline]
    pub const fn has(&self, bytes: usize) -> bool {
        self.offset + bytes <= self.size
    }

    #[inline]
    pub fn allocate(&mut self, bytes: usize) -> *mut u8 {
        unsafe {
            let result = self.data.offset(self.offset as isize);
            self.offset += bytes;
            result
        }
    }
}

impl Drop for ZoneBlock {
    fn drop(&mut self) {
        unsafe {
            if !self.data.is_null() {
                free(self.data);
            }
        }
    }
}
use crate::utils::roundup;

pub static mut ZONE: Option<*mut Zone> = None;

pub const PAGE_SIZE: usize = 4096;

pub struct Zone {
    parent: *mut Zone,
    blocks: Vec<Box<ZoneBlock>>,
    page_size: usize,
}

pub fn init_zonealloc() {
    unsafe {
        if ZONE.is_none() {
            ZONE = Some(std::ptr::null_mut());
        }
        let zone = Zone::new();

        ZONE = Some(Box::into_raw(zone));
    }
}

#[inline]
pub fn zalloc<T: Sized>() -> *mut T {
    unsafe {
        let zone = ZONE.unwrap();
        (*zone).allocate(std::mem::size_of::<T>()) as *mut _
    }
}

#[inline]
pub fn zalloc_raw(bytes: usize) -> *mut u8 {
    unsafe {
        let zone = ZONE.unwrap();
        (*zone).allocate(bytes)
    }
}

impl Zone {
    pub fn new() -> Box<Zone> {
        unsafe {
            let ptr = malloc(std::mem::size_of::<Zone>());

            let mut zone: Box<Zone> = Box::from_raw(ptr as *mut Zone);
            zone.parent = ZONE.unwrap();
            zone.page_size = PAGE_SIZE;
            let box_ptr = malloc(std::mem::size_of::<ZoneBlock>()) as *mut ZoneBlock;
            let mut block = Box::from_raw(box_ptr);
            block.data = malloc(10 * zone.page_size);
            block.size = 10 * zone.page_size;

            zone.blocks.push(block);
            zone
        }
    }

    pub fn allocate(&mut self, bytes: usize) -> *mut u8 {
        if self.blocks.last().unwrap().has(bytes) {
            return self.blocks.last_mut().unwrap().allocate(bytes);
        } else {
            unsafe {
                let block_ptr = malloc(std::mem::size_of::<ZoneBlock>());
                let mut block = Box::from_raw(block_ptr as *mut ZoneBlock);
                block.data = malloc(roundup(bytes as _, self.page_size as _) as _);
                block.offset = 0;
                block.size = roundup(bytes as _, self.page_size as _) as usize;
                let result = block.allocate(bytes);
                self.blocks.insert(0, block);
                return result;
            }
        }
    }
}
