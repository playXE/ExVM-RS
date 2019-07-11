extern "C" {
    fn malloc(x: usize) -> *mut u8;
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Page {
    pub(super) data: *mut u8,
    pub(super) top: *mut u8,
    pub(super) limit: *mut u8,
    pub(super) size: usize,
}

pub const PAGE_SIZE: usize = 4096;

impl Page {
    #[inline]
    pub fn new(x: usize) -> Page {
        let data = unsafe { malloc(x) };
        Page {
            size: x,
            data,
            top: unsafe { data.offset(1) },
            limit: unsafe { data.offset(x as isize) },
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Space {
    pub top: *mut *mut u8,
    pub limit: *mut *mut u8,
    pub pages: Vec<Page>,
    pub page_size: usize,
    pub size: usize,
    pub size_limit: usize,
}

impl Space {
    pub fn new(page_size: usize) -> Space {
        let mut space = Space {
            page_size,
            size: 0,
            pages: vec![],
            top: std::ptr::null_mut(),
            limit: std::ptr::null_mut(),
            size_limit: 0,
        };

        let page = Page::new(page_size);

        space.select(&page);
        space.pages.push(page);

        space
    }

    pub fn compute_size_limit(&mut self) {
        self.size_limit = self.size << 1;
    }

    pub fn select(&mut self, page: &Page) {
        self.top = (&page.top) as *const *mut u8 as *mut *mut _;
        self.limit = (&page.limit) as *const *mut u8 as *mut *mut _;
    }

    pub fn allocate(&mut self, bytes: usize) -> *mut u8 {
        assert!(bytes != 0);
        let even_bytes = bytes + (bytes & 0x01);

        unsafe {
            let place_in_current = (*self.top).offset(even_bytes as _) <= *self.limit;
            if !place_in_current {
                let mut i = 0;
                let mut gap_found = false;
                for item in self.pages.clone().iter() {
                    if (*self.top).offset(even_bytes as _) > *self.limit {
                        if i < self.pages.len() {
                            gap_found = true;
                        } else {
                            gap_found = false;
                        }
                        i = i + 1;
                        self.select(&item);
                    } else {
                        break;
                    }
                }

                if !gap_found {
                    if self.size > self.size_limit {}
                    self.add_page(even_bytes + 1);
                }
            }
            let result = *self.top;
            (*self.top) = (*self.top).offset(even_bytes as _);
            return result;
        }
    }

    pub fn clear(&mut self) {
        self.pages.clear();
    }

    pub fn add_page(&mut self, size: usize) {
        let real_size = crate::utils::roundup(size as _, self.page_size as _) as usize;

        let page = Page::new(real_size);
        self.size += real_size;
        self.select(&page);
        self.pages.push(page);
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(u8)]
pub enum HeapTag {
    Nil = 0x01,
    Context,
    Boolean,
    Number,
    String,
    Object,
    Array,
    Function,
    ExternData,
    Map,
}
#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(u8)]
pub enum Tenure {
    New = 0,
    Old = 1,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(u8)]
pub enum GCType {
    None = 0,
    NewSpace = 1,
    OldSpace = 2,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(u8)]
pub enum RefType {
    Weak,
    Persistent,
}

pub const MIN_OLD_SPACE_GEN: u8 = 5;
pub const MIN_FACTORY_SIZE: u8 = 128;
pub const ENTER_FRAME_TAG: usize = 0xFEEDBEEE;
pub const BINDING_CONTEXT_TAG: usize = 0x0DEC0DEC;
pub const IC_DISABLED_VALUE: usize = 0xABBAABBA;
pub const IC_ZAP_VALUE: usize = 0xABBADEEC;

pub trait HValTrait: Sized + Copy {
    fn addr(&self) -> *mut u8 {
        unsafe { std::mem::transmute(self) }
    }

    const TAG: HeapTag;
}

const fn interior_offset(x: isize) -> isize {
    return x * std::mem::size_of::<isize>() as isize - 1;
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub struct HValue;

impl HValTrait for HValue {
    const TAG: HeapTag = HeapTag::Nil;
}

impl HValue {
    pub const TAG_OFFSET: isize = interior_offset(0);
    pub const GC_MARK_OFF: isize = interior_offset(1) - 1;
    pub const GC_FORWARD_OFF: isize = interior_offset(1);
    pub const REPR_OFF: isize = interior_offset(0) + 1;
    pub const GENERATION_OFF: isize = interior_offset(0) + 2;

    pub fn is_soft_gc_marked(&self) -> bool {
        if Self::is_unboxed(self.addr()) {
            return false;
        }
        unsafe {
            return (*self.addr().offset(HValue::GC_MARK_OFF)) & 0x40 != 0;
        }
    }

    pub fn set_soft_gc_mark(&self) {
        unsafe {
            *(self.addr().offset(Self::GC_MARK_OFF)) |= 0x40;
            //*(self.addr().offset(Self::GC_FORWARD_OFF) as *mut *mut u8) = new_addr;
        }
    }

    pub fn reset_soft_gc_mark(&self) {
        unsafe {
            if self.is_soft_gc_marked() {
                *(self.addr().offset(Self::GC_MARK_OFF)) ^= 0x40;
            }
        }
    }

    #[inline]
    pub fn is_gc_marked(&self) -> bool {
        if Self::is_unboxed(self.addr()) {
            return false;
        }
        unsafe {
            return (*self.addr().offset(HValue::GC_MARK_OFF)) & 0x80 != 0;
        }
    }
    #[inline]
    pub const fn is_unboxed(addr: *mut u8) -> bool {
        return unsafe { (addr as usize & 0x01) == 0 };
    }
    #[inline]
    pub const fn cast(addr: *mut u8) -> *mut HValue {
        return addr as *mut HValue;
    }

    pub fn tag(&self) -> HeapTag {
        Self::get_tag(self.addr())
    }
    pub fn as_<T: HValTrait>(&self) -> *mut T {
        assert!(self.tag() == T::TAG);
        return unsafe { std::mem::transmute(self) };
    }

    pub fn get_tag(addr: *mut u8) -> HeapTag {
        if addr == (HeapTag::Nil as u8 as *mut u8) {
            return HeapTag::Nil;
        }

        if Self::is_unboxed(addr) {
            return HeapTag::Number;
        }

        return unsafe { std::mem::transmute(*addr.offset(Self::TAG_OFFSET)) };
    }

    pub fn get_repr(addr: *mut u8) -> u8 {
        return unsafe { std::mem::transmute(*(addr.offset(Self::REPR_OFF))) };
    }

    pub fn get_gc_mark(&self) -> *mut u8 {
        return unsafe { *(self.addr().offset(Self::GC_FORWARD_OFF) as *mut *mut u8) };
    }

    pub fn is_marked(&self) -> bool {
        if HValue::is_unboxed(self.addr()) {
            return false;
        }

        return unsafe { (*self.addr().offset(Self::GC_MARK_OFF) & 0x80) != 0 };
    }

    pub fn set_gc_mark(&self, new_addr: *mut u8) {
        unsafe {
            *(self.addr().offset(Self::GC_MARK_OFF)) |= 0x80;
            *(self.addr().offset(Self::GC_FORWARD_OFF) as *mut *mut u8) = new_addr;
        }
    }

    pub fn size(&self) -> usize {
        const PTR_SIZE: usize = 8;
        unsafe {
            let mut size = PTR_SIZE;
            match self.tag() {
                HeapTag::Context => {
                    size += (2 * (*self.as_::<HContext>()).slots() as usize) * PTR_SIZE;
                }
                HeapTag::Function => {
                    size += 4 * PTR_SIZE;
                }
                HeapTag::Number => {
                    size += 8;
                }

                HeapTag::Boolean => {
                    size += 8;
                }

                HeapTag::String => {
                    size += 2 * PTR_SIZE;
                    match Self::get_repr(self.addr()) {
                        0 => {
                            size += (*self.as_::<HString>()).length() as usize;
                        }
                        _ => {
                            size += 2 * PTR_SIZE;
                        }
                    }
                }
                HeapTag::Object => {
                    size += 3 * PTR_SIZE;
                }
                HeapTag::Array => {
                    size += 4 * PTR_SIZE;
                }
                HeapTag::Map => {
                    size += (1 + ((*self.as_::<HMap>()).size() as usize) << 1) * PTR_SIZE;
                }

                _ => (),
            }

            size
        }
    }

    pub fn copy_to(&self, addr: &mut crate::gc::Address) -> (*mut u8, usize) {
        const PTR_SIZE: usize = 8;
        unsafe {
            let mut size = PTR_SIZE;
            match self.tag() {
                HeapTag::Context => {
                    size += (2 * (*self.as_::<HContext>()).slots() as usize) * PTR_SIZE;
                }
                HeapTag::Function => {
                    size += 4 * PTR_SIZE;
                }
                HeapTag::Number => {
                    size += 8;
                }

                HeapTag::Boolean => {
                    size += 8;
                }

                HeapTag::String => {
                    size += 2 * PTR_SIZE;
                    match Self::get_repr(self.addr()) {
                        0 => {
                            size += (*self.as_::<HString>()).length() as usize;
                        }
                        _ => {
                            size += 2 * PTR_SIZE;
                        }
                    }
                }
                HeapTag::Object => {
                    size += 3 * PTR_SIZE;
                }
                HeapTag::Array => {
                    size += 4 * PTR_SIZE;
                }
                HeapTag::Map => {
                    size += (1 + ((*self.as_::<HMap>()).size() as usize) << 1) * PTR_SIZE;
                }

                _ => unimplemented!(),
            }
            let result = self.addr().offset(interior_offset(0));
            std::ptr::copy_nonoverlapping(
                result,
                addr.to_mut_ptr::<u8>().offset(interior_offset(0)),
                size,
            );

            return (result, size);
        }
    }
}
#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub struct HContext;

impl HValTrait for HContext {
    const TAG: HeapTag = HeapTag::Context;
}

impl HContext {
    pub fn parent_slot(&self) -> *mut *mut u8 {
        return unsafe { (self.addr().offset(Self::PARENT_OFFSET)) as *mut *mut _ };
    }

    pub fn parent(&self) -> *mut u8 {
        unsafe { *self.parent_slot() }
    }

    pub fn has_parent(&self) -> bool {
        !self.parent().is_null()
    }

    pub fn slots(&self) -> u32 {
        return unsafe { *(self.addr().offset(Self::SLOTS_OFFSET) as *mut u32) };
    }

    pub fn get_slot(&self, idx: u32) -> *mut HValue {
        unsafe { HValue::cast(*self.get_slot_address(idx)) }
    }

    pub fn has_slot(&self, idx: u32) -> bool {
        return unsafe { *self.get_slot_address(idx) != HeapTag::Nil as u8 as *mut u8 };
    }

    pub fn get_slot_address(&self, idx: u32) -> *mut *mut u8 {
        return unsafe { (self.addr().offset(Self::get_index_disp(idx))) as *mut *mut u8 };
    }

    pub fn get_index_disp(index: u32) -> isize {
        return interior_offset(3 + index as isize);
    }

    pub const PARENT_OFFSET: isize = interior_offset(1);
    pub const SLOTS_OFFSET: isize = interior_offset(2);
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
#[repr(u8)]
pub enum StrRepr {
    Normal = 0x00,
    Cons = 0x01,
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub struct HString;

impl HValTrait for HString {
    const TAG: HeapTag = HeapTag::String;
}

impl HString {
    pub const HASH_OFFSET: isize = interior_offset(1);
    pub const LENGTH_OFFSET: isize = interior_offset(2);
    pub const VALUE_OFFSET: isize = interior_offset(3);
    pub const LEFT_CONS_OFFSET: isize = interior_offset(3);
    pub const RIGHT_CONS_OFFSET: isize = interior_offset(4);
    pub const MIN_CONS_LEN: usize = 24;

    pub fn static_length(addr: *mut u8) -> u32 {
        return unsafe { *(addr.offset(HString::LENGTH_OFFSET) as *mut u32) };
    }

    pub fn length(&self) -> u32 {
        Self::static_length(self.addr())
    }
}
#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub struct HMap;

impl HValTrait for HMap {
    const TAG: HeapTag = HeapTag::Map;
}

impl HMap {
    pub fn size(&self) -> u32 {
        return unsafe { *(self.addr().offset(Self::SIZE_OFFSET) as *mut u32) };
    }

    pub fn get_slot_address(&self, index: u32) -> *mut *mut u8 {
        return unsafe {
            self.space()
                .offset(index as isize * crate::mem::ptr_width() as isize)
                as *mut *mut _
        };
    }

    pub fn get_slot(&self, index: u32) -> *mut HValue {
        return unsafe { HValue::cast(*self.get_slot_address(index)) };
    }

    pub fn has_slot(&self, index: u32) -> bool {
        unsafe { *self.get_slot_address(index) != HeapTag::Nil as u8 as *mut u8 }
    }

    pub fn space(&self) -> *mut u8 {
        unsafe { self.addr().offset(Self::SPACE_OFFSET) }
    }
    pub const SPACE_OFFSET: isize = interior_offset(2);
    pub const SIZE_OFFSET: isize = interior_offset(1);
}
#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub struct HArray;

impl HValTrait for HArray {
    const TAG: HeapTag = HeapTag::Array;
}
#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub struct HNumber;

impl HNumber {
    pub const fn tag(value: i64) -> i64 {
        return value << 1;
    }
}

impl HArray {
    pub fn length(obj: *mut u8, shrink: bool) -> usize {
        unsafe {
            let mut result = *(obj.offset(Self::LENGTH_OFFSET) as *mut usize);
            if shrink {
                let mut shrinked = result;
                let shrinked_ptr: *mut u8;
                let slot: *mut *mut u8;

                if shrinked < 0 {
                } else {
                    shrinked -= 1;
                    shrinked_ptr = HNumber::tag(shrinked as i64) as *mut u8;
                }

                if result != (shrinked + 1) {
                    result = shrinked + 1;
                    HArray::set_length(obj, result);
                }
            }

            result
        }
    }

    pub fn set_length(obj: *mut u8, len: usize) {
        unsafe {
            *(obj.offset(Self::LENGTH_OFFSET) as *mut usize) = len;
        }
    }

    pub const VAR_ARG_LEN: usize = 16;
    pub const DENSE_LENGTH_MAX: usize = 128;
    pub const LENGTH_OFFSET: isize = interior_offset(4);
}
#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub struct HObject;

impl HValTrait for HObject {
    const TAG: HeapTag = HeapTag::Object;
}

impl HObject {
    pub fn map_slot_s(addr: *mut u8) -> *mut *mut u8 {
        return unsafe { addr.offset(Self::MAP_OFFSET) as *mut *mut _ };
    }

    pub fn map_s(addr: *mut u8) -> *mut u8 {
        return unsafe { *Self::map_slot_s(addr) };
    }

    pub fn map(&self) -> *mut u8 {
        Self::map_s(self.addr())
    }

    pub fn map_slot(&self) -> *mut *mut u8 {
        Self::map_slot_s(self.addr())
    }

    pub fn proto_slot_s(addr: *mut u8) -> *mut *mut u8 {
        return unsafe { addr.offset(Self::PROTO_OFFSET) as *mut *mut _ };
    }

    pub fn proto_s(addr: *mut u8) -> *mut u8 {
        unsafe { *Self::proto_slot_s(addr) }
    }

    pub fn proto(&self) -> *mut u8 {
        Self::proto_s(self.addr())
    }

    pub fn proto_slot(&self) -> *mut *mut u8 {
        Self::proto_slot_s(self.addr())
    }

    pub const MASK_OFFSET: isize = interior_offset(1);
    pub const MAP_OFFSET: isize = interior_offset(2);
    pub const PROTO_OFFSET: isize = interior_offset(3);
}
#[derive(Copy, Clone, Debug, Hash, PartialEq, PartialOrd, Ord, Eq)]
pub struct HFunction;

impl HValTrait for HFunction {
    const TAG: HeapTag = HeapTag::Function;
}

impl HFunction {
    pub const PARENT_OFFSET: isize = interior_offset(1);
    pub const CODE_OFFSET: isize = interior_offset(2);
    pub const ROOT_OFFSET: isize = interior_offset(3);
    pub const ARGC_OFFSET: isize = interior_offset(4);

    pub fn root_s(addr: *mut u8) -> *mut u8 {
        unsafe { *(addr.offset(Self::ROOT_OFFSET) as *mut *mut u8) }
    }

    pub fn root_slot(&self) -> *mut *mut u8 {
        unsafe { self.addr().offset(Self::ROOT_OFFSET) as *mut *mut u8 }
    }

    pub fn root(&self) -> *mut u8 {
        unsafe { *(self.root_slot()) }
    }

    pub fn argc(&self) -> u32 {
        unsafe { *self.argc_off() }
    }

    pub fn parent(&self) -> *mut u8 {
        unsafe { *self.parent_slot() }
    }

    pub fn parent_slot(&self) -> *mut *mut u8 {
        unsafe { self.addr().offset(Self::PARENT_OFFSET) as *mut *mut _ }
    }

    pub fn argc_off(&self) -> *mut u32 {
        unsafe { self.addr().offset(Self::ARGC_OFFSET) as *mut u32 }
    }
}
