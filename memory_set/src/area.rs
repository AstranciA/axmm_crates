use core::fmt;

use memory_addr::{AddrRange, MemoryAddr, PAGE_SIZE_4K};

use crate::{MappingBackend, MappingError, MappingResult};
use alloc::collections::BTreeMap;


pub struct AreaStat {
    pub start: usize,
    pub end: usize,
    pub size: usize,
    pub rss: usize,
    pub swap: usize,
}

/// A memory area represents a continuous range of virtual memory with the same
/// flags.
///
/// The target physical memory frames are determined by [`MappingBackend`] and
/// may not be contiguous.
#[derive(Clone)]
pub struct MemoryArea<B: MappingBackend> {
    va_range: AddrRange<B::Addr>,
    /// Hold pages with RAII.
    /// The key is the vpn of the page,
    /// so it must be aligned to PAGE_SIZE_4K.
    #[cfg(feature = "RAII")]
    pub frames: BTreeMap<B::Addr, B::FrameTrackerRef>,
    flags: B::Flags,
    pub(crate) backend: B,
}

// TODO: should decrease ref of page if mapping is changed.

impl<B: MappingBackend> MemoryArea<B> {
    /// Creates a new memory area.
    ///
    /// # Panics
    ///
    /// Panics if `start + size` overflows.
    pub fn new(
        start: B::Addr,
        size: usize,
        #[cfg(feature = "RAII")] frame_alloced: Option<BTreeMap<B::Addr, B::FrameTrackerRef>>,
        flags: B::Flags,
        backend: B,
    ) -> Self {
        Self {
            va_range: AddrRange::from_start_size(start, size),
            #[cfg(feature = "RAII")]
            frames: frame_alloced.unwrap_or(BTreeMap::new()),
            flags,
            backend,
        }
    }

    pub fn clone_(&self, flags: B::Flags) -> Self {
        let mut area = self.clone();
        area.set_flags(flags);
        area
    }

    /// Returns the virtual address range.
    pub const fn va_range(&self) -> AddrRange<B::Addr> {
        self.va_range
    }

    /// Returns the memory flags, e.g., the permission bits.
    pub const fn flags(&self) -> B::Flags {
        self.flags
    }

    /// Returns the start address of the memory area.
    pub const fn start(&self) -> B::Addr {
        self.va_range.start
    }

    /// Returns the end address of the memory area.
    pub const fn end(&self) -> B::Addr {
        self.va_range.end
    }

    /// Returns the size of the memory area.
    pub fn size(&self) -> usize {
        self.va_range.size()
    }

    /// Returns the mapping backend of the memory area.
    pub const fn backend(&self) -> &B {
        &self.backend
    }

    pub fn stat(&self) -> AreaStat {
        AreaStat {
            start: self.start().into(),
            end: self.end().into(),
            size: self.size(),
            rss: self.frames_count() * PAGE_SIZE_4K, // TODO: large page
            swap: 0
        }
    }
}

#[allow(unused)]
impl<B: MappingBackend> MemoryArea<B> {
    /// Changes the flags.
    pub(crate) fn set_flags(&mut self, new_flags: B::Flags) {
        self.flags = new_flags;
    }

    /// Changes the end address of the memory area.
    pub(crate) fn set_end(&mut self, new_end: B::Addr) {
        self.va_range.end = new_end;
        #[cfg(feature = "RAII")]
        self.retain_frames_in_range();
    }

    /// Maps the whole memory area in the page table.
    pub fn map_area(
        &mut self,
        page_table: &mut B::PageTable,
        flags: Option<B::Flags>,
    ) -> MappingResult {
        let flag = flags.unwrap_or(self.flags);
        let frame_refs = self
            .backend
            .map(self.start(), self.size(), flag, page_table)
            .or(Err(MappingError::BadState))?;
        #[cfg(feature = "RAII")]
        self.frames.extend(frame_refs);
        Ok(())
    }

    /// Unmaps the whole memory area in the page table.
    pub fn unmap_area(&mut self, page_table: &mut B::PageTable) -> MappingResult {
        // Backend::Unmap will not deallocate the frames if feature = "RAII".
        self.backend
            .unmap(self.start(), self.size(), page_table)
            .then_some(())
            .ok_or(MappingError::BadState)?;
        // Decrease the ref of frame trackers.
        #[cfg(feature = "RAII")]
        self.frames.clear();
        Ok(())
    }

    pub fn unmap_frames(
        &mut self,
        start: B::Addr,
        size: usize,
        page_table: &mut B::PageTable,
    ) -> MappingResult {
        // Backend::Unmap will not deallocate the frames if feature = "RAII".
        self.backend
            .unmap(start, size, page_table)
            .then_some(())
            .ok_or(MappingError::BadState)?;
        // Decrease the ref of frame trackers.
        #[cfg(feature = "RAII")]
        {
            let mut tail = self.frames.split_off(&start);
            self.frames.append(&mut tail.split_off(&(start.add(size))));
        }
        Ok(())
    }

    /// Changes the flags in the page table.
    pub(crate) fn protect_area(
        &mut self,
        new_flags: B::Flags,
        page_table: &mut B::PageTable,
    ) -> MappingResult {
        self.backend
            .protect(self.start(), self.size(), new_flags, page_table);
        Ok(())
    }

    /// Shrinks the memory area at the left side.
    ///
    /// The start address of the memory area is increased by `new_size`. The
    /// shrunk part is unmapped.
    ///
    /// `new_size` must be greater than 0 and less than the current size.
    pub(crate) fn shrink_left(
        &mut self,
        new_size: usize,
        page_table: &mut B::PageTable,
    ) -> MappingResult {
        assert!(new_size > 0 && new_size < self.size());

        let old_size = self.size();
        let unmap_size = old_size - new_size;

        if !self.backend.unmap(self.start(), unmap_size, page_table) {
            return Err(MappingError::BadState);
        }
        // Use wrapping_add to avoid overflow check.
        // Safety: `unmap_size` is less than the current size, so it will never
        // overflow.
        self.va_range.start = self.va_range.start.wrapping_add(unmap_size);
        #[cfg(feature = "RAII")]
        self.retain_frames_in_range();

        Ok(())
    }

    /// Shrinks the memory area at the right side.
    ///
    /// The end address of the memory area is decreased by `new_size`. The
    /// shrunk part is unmapped.
    ///
    /// `new_size` must be greater than 0 and less than the current size.
    pub(crate) fn shrink_right(
        &mut self,
        new_size: usize,
        page_table: &mut B::PageTable,
    ) -> MappingResult {
        assert!(new_size > 0 && new_size < self.size());
        let old_size = self.size();
        let unmap_size = old_size - new_size;

        // Use wrapping_add to avoid overflow check.
        // Safety: `new_size` is less than the current size, so it will never overflow.
        let unmap_start = self.start().wrapping_add(new_size);

        if !self.backend.unmap(unmap_start, unmap_size, page_table) {
            return Err(MappingError::BadState);
        }

        // Use wrapping_sub to avoid overflow check, same as above.
        self.va_range.end = self.va_range.end.wrapping_sub(unmap_size);
        #[cfg(feature = "RAII")]
        self.retain_frames_in_range();
        Ok(())
    }
    ///WARN: 直接调用可能会导致areas重叠
    pub(crate) unsafe fn extend_left(
        &mut self,
        new_size: usize,
        page_table: &mut B::PageTable,
    ) -> MappingResult {
        assert!(new_size > 0 && new_size > self.size());
        let map_size = new_size - self.size();
        let map_start = self.start().wrapping_sub(map_size);
        let map_result = self
            .backend
            .map(map_start, map_size, self.flags, page_table);

        #[cfg(feature = "RAII")]
        {
            let mut new_frames = match map_result {
                Ok(r) => r,
                Err(_) => return Err(MappingError::BadState),
            };
            self.frames.append(&mut new_frames);
        }
        #[cfg(not(feature = "RAII"))]
        if map_result.is_err() {
            return Err(MappingError::BadState);
        }
        self.va_range.start = map_start;
        Ok(())
    }

    pub(crate) unsafe fn extend_right(
        &mut self,
        new_size: usize,
        page_table: &mut B::PageTable,
    ) -> MappingResult {
        assert!(new_size > 0 && new_size > self.size());
        let map_size = new_size - self.size();
        let map_start = self.start().wrapping_add(self.size());
        let map_result = self
            .backend
            .map(map_start, map_size, self.flags, page_table);

        #[cfg(feature = "RAII")]
        {
            let mut new_frames = match map_result {
                Ok(r) => r,
                Err(_) => return Err(MappingError::BadState),
            };
            self.frames.append(&mut new_frames);
        }
        #[cfg(not(feature = "RAII"))]
        if map_result.is_err() {
            return Err(MappingError::BadState);
        }
        self.va_range.end = self.va_range.end.wrapping_add(map_size);
        Ok(())
    }

    /// Splits the memory area at the given position.
    ///
    /// The original memory area is shrunk to the left part, and the right part
    /// is returned.
    ///
    /// Returns `None` if the given position is not in the memory area, or one
    /// of the parts is empty after splitting.
    pub fn split(&mut self, pos: B::Addr) -> Option<Self> {
        if self.start() < pos && pos < self.end() {
            let new_area = Self::new(
                pos,
                // Use wrapping_sub_addr to avoid overflow check. It is safe because
                // `pos` is within the memory area.
                self.end().wrapping_sub_addr(pos),
                #[cfg(feature = "RAII")]
                Some(self.frames.split_off(&pos)), // pages retained here
                self.flags,
                self.backend.clone(),
            );
            self.va_range.end = pos;
            // already retained
            //self.retain_pages_in_range();
            Some(new_area)
        } else {
            None
        }
    }
}
#[cfg(feature = "RAII")]
impl<B: MappingBackend> MemoryArea<B> {
    /// Inserts a frame into the memory area.
    /// Frame will be replaced if vaddr already in frame maps.
    pub fn insert_frame(
        &mut self,
        vaddr: B::Addr,
        frame: B::FrameTrackerRef,
    ) -> Option<<B as MappingBackend>::FrameTrackerRef> {
        debug_assert!(vaddr.is_aligned_4k());
        self.frames.insert(vaddr, frame)
    }

    pub fn find_frame(&self, vaddr: B::Addr) -> Option<B::FrameTrackerRef> {
        debug_assert!(vaddr.is_aligned_4k());
        self.frames.get(&vaddr).cloned()
    }

    pub fn frames_count(&self) -> usize {
        self.frames.len()
    }

    /// Retains only the pages in [self.va_range].
    /// called manually when the va_range is changed.
    fn retain_frames_in_range(&mut self) {
        let range = self.va_range();
        self.frames.retain(|&frame, _| range.contains(frame));
    }
}

#[cfg(feature = "mmap")]
impl<B: MappingBackend> MemoryArea<B> {
    pub fn new_mmap(
        start: B::Addr,
        size: usize,
        frame_alloced: Option<BTreeMap<B::Addr, B::FrameTrackerRef>>,
        flags: B::Flags,
        backend: B,
    ) -> Self {
        Self {
            va_range: AddrRange::from_start_size(start, size),
            frames: frame_alloced.unwrap_or(BTreeMap::new()),
            flags,
            backend,
        }
    }
}

impl<B: MappingBackend> fmt::Debug for MemoryArea<B>
where
    B::Addr: fmt::Debug,
    B::Flags: fmt::Debug + Copy,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("MemoryArea")
            .field("va_range", &self.va_range)
            .field("flags", &self.flags)
            .finish()
    }
}
