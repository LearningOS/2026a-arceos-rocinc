#![no_std]

use allocator::{AllocError, AllocResult, BaseAllocator, ByteAllocator, PageAllocator};
use core::alloc::Layout;
use core::ptr::NonNull;

/// Early memory allocator
/// Use it before formal bytes-allocator and pages-allocator can work!
/// This is a double-end memory range:
/// - Alloc bytes forward
/// - Alloc pages backward
///
/// [ bytes-used | avail-area | pages-used ]
/// |            | -->    <-- |            |
/// start       b_pos        p_pos       end
///
/// For bytes area, 'count' records number of allocations.
/// When it goes down to ZERO, free bytes-used area.
/// For pages area, it will never be freed!
///
pub struct EarlyAllocator<const SIZE: usize> {
    start: usize,
    end: usize,
    b_pos: usize,
    p_pos: usize,
    count: usize,
}

#[inline]
const fn align_down(pos: usize, align: usize) -> usize {
    pos & !(align - 1)
}

#[inline]
const fn align_up(pos: usize, align: usize) -> Option<usize> {
    match pos.checked_add(align - 1) {
        Some(v) => Some(v & !(align - 1)),
        None => None,
    }
}

impl<const SIZE: usize> EarlyAllocator<SIZE> {
    pub const fn new() -> Self {
        Self {
            start: 0,
            end: 0,
            b_pos: 0,
            p_pos: 0,
            count: 0,
        }
    }
}

impl<const SIZE: usize> Default for EarlyAllocator<SIZE> {
    fn default() -> Self {
        Self::new()
    }
}

impl<const SIZE: usize> BaseAllocator for EarlyAllocator<SIZE> {
    fn init(&mut self, start: usize, size: usize) {
        assert!(SIZE.is_power_of_two());
        self.start = start;
        self.end = start + size;
        self.b_pos = start;
        self.p_pos = self.end;
        self.count = 0;
    }

    fn add_memory(&mut self, start: usize, size: usize) -> AllocResult {
        if size == 0 {
            return Err(AllocError::InvalidParam);
        }
        let new_end = start.checked_add(size).ok_or(AllocError::InvalidParam)?;
        if self.start == 0 && self.end == 0 {
            self.init(start, size);
            return Ok(());
        }
        if start < self.end && new_end > self.start {
            return Err(AllocError::MemoryOverlap);
        }
        if start == self.end && self.p_pos == self.end {
            self.end = new_end;
            self.p_pos = new_end;
            return Ok(());
        }
        Err(AllocError::NoMemory)
    }
}

impl<const SIZE: usize> ByteAllocator for EarlyAllocator<SIZE> {
    fn alloc(&mut self, layout: Layout) -> AllocResult<NonNull<u8>> {
        let alloc_start = align_up(self.b_pos, layout.align()).ok_or(AllocError::NoMemory)?;
        let alloc_end = alloc_start
            .checked_add(layout.size())
            .ok_or(AllocError::NoMemory)?;
        if alloc_end > self.p_pos {
            return Err(AllocError::NoMemory);
        }
        let ptr = NonNull::new(alloc_start as *mut u8).ok_or(AllocError::NoMemory)?;
        self.b_pos = alloc_end;
        self.count += 1;
        Ok(ptr)
    }

    fn dealloc(&mut self, _pos: NonNull<u8>, _layout: Layout) {
        self.count = self.count.saturating_sub(1);
        if self.count == 0 {
            self.b_pos = self.start;
        }
    }

    fn total_bytes(&self) -> usize {
        self.p_pos - self.start
    }

    fn used_bytes(&self) -> usize {
        self.b_pos - self.start
    }

    fn available_bytes(&self) -> usize {
        self.p_pos - self.b_pos
    }
}

impl<const SIZE: usize> PageAllocator for EarlyAllocator<SIZE> {
    const PAGE_SIZE: usize = SIZE;

    fn alloc_pages(&mut self, num_pages: usize, align_pow2: usize) -> AllocResult<usize> {
        if num_pages == 0
            || !SIZE.is_power_of_two()
            || !align_pow2.is_power_of_two()
            || align_pow2 < SIZE
        {
            return Err(AllocError::InvalidParam);
        }
        let alloc_size = num_pages.checked_mul(SIZE).ok_or(AllocError::NoMemory)?;
        let new_p_pos = align_down(
            self.p_pos
                .checked_sub(alloc_size)
                .ok_or(AllocError::NoMemory)?,
            align_pow2,
        );
        if new_p_pos < self.b_pos {
            return Err(AllocError::NoMemory);
        }
        self.p_pos = new_p_pos;
        Ok(new_p_pos)
    }

    fn dealloc_pages(&mut self, _pos: usize, _num_pages: usize) {}

    fn total_pages(&self) -> usize {
        (self.end - self.b_pos) / SIZE
    }

    fn used_pages(&self) -> usize {
        (self.end - self.p_pos) / SIZE
    }

    fn available_pages(&self) -> usize {
        (self.p_pos - self.b_pos) / SIZE
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE_SIZE: usize = 0x1000;

    #[test]
    fn double_ended_alloc_and_reset() {
        let mut alloc = EarlyAllocator::<PAGE_SIZE>::new();
        let base = 0x8000_0000usize;
        alloc.init(base, 4 * PAGE_SIZE);

        assert_eq!(alloc.total_bytes(), 4 * PAGE_SIZE);
        assert_eq!(alloc.total_pages(), 4);

        let p1 = alloc
            .alloc(Layout::from_size_align(100, 8).unwrap())
            .unwrap();
        assert_eq!(p1.as_ptr() as usize, base);
        assert_eq!(alloc.used_bytes(), 100);

        let p2 = alloc
            .alloc(Layout::from_size_align(64, 64).unwrap())
            .unwrap();
        assert_eq!(p2.as_ptr() as usize, base + 128);
        assert_eq!(alloc.used_bytes(), 192);

        let pg1 = alloc.alloc_pages(1, PAGE_SIZE).unwrap();
        assert_eq!(pg1, base + 3 * PAGE_SIZE);
        assert_eq!(alloc.used_pages(), 1);
        assert_eq!(alloc.available_pages(), 2);
        assert_eq!(
            alloc.total_bytes(),
            alloc.used_bytes() + alloc.available_bytes()
        );

        let pg2 = alloc.alloc_pages(1, 2 * PAGE_SIZE).unwrap();
        assert_eq!(pg2, base + 2 * PAGE_SIZE);
        assert_eq!(alloc.used_pages(), 2);

        assert!(matches!(
            alloc.alloc_pages(2, PAGE_SIZE),
            Err(AllocError::NoMemory)
        ));
        assert!(matches!(
            alloc.alloc(Layout::from_size_align(2 * PAGE_SIZE, 8).unwrap()),
            Err(AllocError::NoMemory)
        ));
        assert!(matches!(
            alloc.alloc_pages(0, PAGE_SIZE),
            Err(AllocError::InvalidParam)
        ));
        assert!(matches!(
            alloc.alloc_pages(1, PAGE_SIZE / 2),
            Err(AllocError::InvalidParam)
        ));
        assert!(matches!(
            alloc.alloc_pages(1, 3 * PAGE_SIZE),
            Err(AllocError::InvalidParam)
        ));

        alloc.dealloc_pages(pg2, 1);
        assert_eq!(alloc.used_pages(), 2);

        alloc.dealloc(p1, Layout::from_size_align(100, 8).unwrap());
        assert_eq!(alloc.used_bytes(), 192);

        alloc.dealloc(p2, Layout::from_size_align(64, 64).unwrap());
        assert_eq!(alloc.used_bytes(), 0);
        assert_eq!(alloc.available_bytes(), 2 * PAGE_SIZE);
        assert_eq!(alloc.available_pages(), 2);
    }
}
