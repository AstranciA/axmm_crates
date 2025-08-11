use alloc::collections::BTreeMap;
#[allow(unused_imports)] // this is a weird false alarm
use alloc::vec::Vec;
use core::fmt;
use memory_addr::{AddrRange, MemoryAddr};

use crate::{MappingBackend, MappingError, MappingResult, MemoryArea};

/// A container that maintains memory mappings ([`MemoryArea`]).
pub struct MemorySet<B: MappingBackend> {
    areas: BTreeMap<B::Addr, MemoryArea<B>>,
}

impl<B: MappingBackend> MemorySet<B> {
    /// Creates a new memory set.
    pub const fn new() -> Self {
        Self {
            areas: BTreeMap::new(),
        }
    }

    /// Returns the number of memory areas in the memory set.
    pub fn len(&self) -> usize {
        self.areas.len()
    }

    /// Returns `true` if the memory set contains no memory areas.
    pub fn is_empty(&self) -> bool {
        self.areas.is_empty()
    }

    /// Returns the iterator over all memory areas.
    pub fn iter(&self) -> impl Iterator<Item = &MemoryArea<B>> {
        self.areas.values()
    }

    /// Returns the iterator over all memory areas.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut MemoryArea<B>> {
        self.areas.values_mut()
    }

    /// Returns whether the given address range overlaps with any existing area.
    pub fn overlaps(&self, range: AddrRange<B::Addr>) -> bool {
        if let Some((_, before)) = self.areas.range(..range.start).last() {
            if before.va_range().overlaps(range) {
                return true;
            }
        }
        if let Some((_, after)) = self.areas.range(range.start..).next() {
            if after.va_range().overlaps(range) {
                return true;
            }
        }
        false
    }

    /// Finds the memory area that contains the given address.
    pub fn find(&self, addr: B::Addr) -> Option<&MemoryArea<B>> {
        self.areas
            .range(..=addr)
            .rev()
            .take_while(|candidate| *candidate.0 <= addr)
            .find(|(_, candidate)| candidate.va_range().contains(addr))
            .map(|(_, area)| area)
    }

    /// Finds the memory area that contains the given address.
    pub fn find_mut(&mut self, addr: B::Addr) -> Option<&mut MemoryArea<B>> {
        let candidate: Option<&mut MemoryArea<B>> =
            self.areas.range_mut(..=addr).last().map(|(_, a)| a);
        candidate.filter(|a| a.va_range().contains(addr))
    }

    /// Finds a free area that can accommodate the given size.
    ///
    /// The search starts from the given `hint` address, and the area should be
    /// within the given `limit` range.
    ///
    /// Returns the start address of the free area. Returns `None` if no such
    /// area is found.
    pub fn find_free_area(
        &self,
        hint: B::Addr,
        size: usize,
        limit: AddrRange<B::Addr>,
    ) -> Option<B::Addr> {
        // brute force: try each area's end address as the start.
        let mut last_end = hint.max(limit.start);
        if let Some((_, area)) = self.areas.range(..last_end).last() {
            last_end = last_end.max(area.end());
        }
        for (&addr, area) in self.areas.range(last_end..) {
            if last_end.checked_add(size).is_some_and(|end| end <= addr) {
                return Some(last_end);
            }
            last_end = area.end();
        }
        if last_end
            .checked_add(size)
            .is_some_and(|end| end <= limit.end)
        {
            Some(last_end)
        } else {
            None
        }
    }

    /// insert an existing memory area into the set.

    /// Add a new memory area without mapping.
    /// Useful for lazy.
    pub fn insert(&mut self, area: MemoryArea<B>, unmap_overlap: bool) -> MappingResult {
        if area.va_range().is_empty() {
            return Err(MappingError::InvalidParam);
        }

        if area.va_range().is_empty() {
            return Err(MappingError::InvalidParam);
        }

        if self.overlaps(area.va_range()) && !unmap_overlap {
            return Err(MappingError::AlreadyExists);
        }
        assert!(self.areas.insert(area.start(), area).is_none());
        Ok(())
    }
    pub fn delete(&mut self, vaddr: B::Addr) {
        self.areas.remove(&vaddr);
    }
    /// Add a new memory mapping.
    ///
    /// The mapping is represented by a [`MemoryArea`].
    ///
    /// If the new area overlaps with any existing area, the behavior is
    /// determined by the `unmap_overlap` parameter. If it is `true`, the
    /// overlapped regions will be unmapped first. Otherwise, it returns an
    /// error.
    pub fn map(
        &mut self,
        mut area: MemoryArea<B>,
        page_table: &mut B::PageTable,
        unmap_overlap: bool,
        overwrite_flags: Option<B::Flags>,
    ) -> MappingResult {
        if area.va_range().is_empty() {
            return Err(MappingError::InvalidParam);
        }

        if self.overlaps(area.va_range()) {
            if unmap_overlap {
                self.unmap(area.start(), area.size(), page_table)?;
            } else {
                return Err(MappingError::AlreadyExists);
            }
        }

        area.map_area(page_table, overwrite_flags)?;
        assert!(self.areas.insert(area.start(), area).is_none());
        Ok(())
    }

    /// Remove memory mappings within the given address range.
    ///
    /// All memory areas that are fully contained in the range will be removed
    /// directly. If the area intersects with the boundary, it will be shrinked.
    /// If the unmapped range is in the middle of an existing area, it will be
    /// split into two areas.
    pub fn unmap(
        &mut self,
        start: B::Addr,
        size: usize,
        page_table: &mut B::PageTable,
    ) -> MappingResult {
        let range =
            AddrRange::try_from_start_size(start, size).ok_or(MappingError::InvalidParam)?;
        if range.is_empty() {
            return Ok(());
        }

        let end = range.end;

        // Unmap entire areas that are contained by the range.
        self.areas.retain(|_, area| {
            if area.va_range().contained_in(range) {
                area.unmap_area(page_table).unwrap();
                false
            } else {
                true
            }
        });

        // Shrink right if the area intersects with the left boundary.
        if let Some((&before_start, before)) = self.areas.range_mut(..start).last() {
            let before_end = before.end();
            if before_end > start {
                if before_end <= end {
                    // the unmapped area is at the end of `before`.
                    before.shrink_right(start.sub_addr(before_start), page_table)?;
                } else {
                    // the unmapped area is in the middle `before`, need to split.
                    let right_part = before.split(end).unwrap();
                    before.shrink_right(start.sub_addr(before_start), page_table)?;
                    assert_eq!(right_part.start().into(), Into::<usize>::into(end));
                    self.areas.insert(end, right_part);
                }
            }
        }

        // Shrink left if the area intersects with the right boundary.
        if let Some((&after_start, after)) = self.areas.range_mut(start..).next() {
            let after_end = after.end();
            if after_start < end {
                // the unmapped area is at the start of `after`.
                let mut new_area = self.areas.remove(&after_start).unwrap();
                new_area.shrink_left(after_end.sub_addr(end), page_table)?;
                assert_eq!(new_area.start().into(), Into::<usize>::into(end));
                self.areas.insert(end, new_area);
            }
        }

        Ok(())
    }

    pub fn adjust_area(
        &mut self,
        area_addr: B::Addr,
        start: B::Addr,
        end: B::Addr,
        page_table: &mut B::PageTable,
    ) -> Result<(), MappingError> {
        let area = self.areas.get_mut(&area_addr).unwrap();
        assert!(start.is_aligned_4k());
        assert!(end.is_aligned_4k());
        let old_range = area.va_range();

        // 检查新的范围是否有效
        if start >= end {
            return Err(MappingError::InvalidParam);
        }

        // 当前区域的边界
        let current_start = old_range.start;
        let current_end = old_range.end;

        // 处理左边界的变化
        if start != current_start {
            if start < current_start {
                // 需要向左扩展
                // 新的总size = (current_end - start)
                unsafe {
                    area.extend_left(current_end.sub_addr(start), page_table)?;
                }
            } else {
                // 需要向右收缩
                // 新的总size = (current_end - start)

                area.shrink_left(current_end.sub_addr(start), page_table)?;
            }
        }

        // 处理右边界的变化
        if end != current_end {
            if end > current_end {
                // 需要向右扩展
                // 新的总size = (end - current_start)
                unsafe {
                    area.extend_right(end.sub_addr(current_start), page_table)?;
                }
            } else {
                // 需要向左收缩
                // 新的总size = (end - current_start)
                area.shrink_right(end.sub_addr(current_start), page_table)?;
            }
        }

        log::debug!("Adjust area range: {old_range:?} -> {:?}", area.va_range());

        let area = self.areas.get_mut(&area_addr).unwrap();
        Ok(())
    }

    /// Remove all memory areas and the underlying mappings.
    pub fn clear(&mut self, page_table: &mut B::PageTable) -> MappingResult {
        for (_, area) in self.areas.iter_mut() {
            area.unmap_area(page_table)?;
        }
        self.areas.clear();
        Ok(())
    }

    /// Change the flags of memory mappings within the given address range.
    ///
    /// `update_flags` is a function that receives old flags and processes
    /// new flags (e.g., some flags can not be changed through this interface).
    /// It returns [`None`] if there is no bit to change.
    ///
    /// Memory areas will be skipped according to `update_flags`. Memory areas
    /// that are fully contained in the range or contains the range or
    /// intersects with the boundary will be handled similarly to `munmap`.
    pub fn protect(
        &mut self,
        start: B::Addr,
        size: usize,
        update_flags: impl Fn(B::Flags) -> Option<B::Flags>,
        page_table: &mut B::PageTable,
    ) -> MappingResult {
        let end = start.checked_add(size).ok_or(MappingError::InvalidParam)?;
        let mut to_insert = Vec::new();
        for (&area_start, area) in self.areas.iter_mut() {
            let area_end = area.end();

            if let Some(new_flags) = update_flags(area.flags()) {
                if area_start >= end {
                    // [ prot ]
                    //          [ area ]
                    break;
                } else if area_end <= start {
                    //          [ prot ]
                    // [ area ]
                    // Do nothing
                } else if area_start >= start && area_end <= end {
                    // [   prot   ]
                    //   [ area ]
                    area.protect_area(new_flags, page_table)?;
                    area.set_flags(new_flags);
                } else if area_start < start && area_end > end {
                    //        [ prot ]
                    // [ left | area | right ]
                    let right_part = area.split(end).unwrap();
                    let mut middle_part = area.split(start).unwrap();

                    middle_part.protect_area(new_flags, page_table)?;
                    middle_part.set_flags(new_flags);

                    to_insert.push((right_part.start(), right_part));
                    to_insert.push((middle_part.start(), middle_part));
                } else if area_end > end {
                    // [    prot ]
                    //   [  area | right ]
                    let right_part = area.split(end).unwrap();
                    area.protect_area(new_flags, page_table)?;
                    area.set_flags(new_flags);

                    to_insert.push((right_part.start(), right_part));
                } else {
                    //        [ prot    ]
                    // [ left |  area ]
                    let mut right_part = area.split(start).unwrap();
                    right_part.protect_area(new_flags, page_table)?;
                    right_part.set_flags(new_flags);

                    to_insert.push((right_part.start(), right_part));
                }
            }
        }
        self.areas.extend(to_insert);
        Ok(())
    }
}

#[cfg(feature = "RAII")]
impl<B: MappingBackend> MemorySet<B> {
    pub fn find_frame(&self, vaddr: B::Addr) -> Option<B::FrameTrackerRef> {
        if let Some(area) = self.find(vaddr) {
            return area.find_frame(vaddr);
        }
        None
    }

    pub fn insert_frame(
        &mut self,
        vaddr: B::Addr,
        frame: B::FrameTrackerRef,
    ) -> Option<B::FrameTrackerRef> {
        if let Some(area) = self.find_mut(vaddr) {
            return area.insert_frame(vaddr.align_down_4k(), frame);
        }
        None
    }

    /// Remap a vaddr to a new frame.pub fn remap_frame(&mut self, vaddr:
    /// B::Addr, new_frame: B::FrameTrackerImpl) {
    pub fn remap_frame(&mut self, vaddr: B::Addr, new_frame: B::FrameTrackerRef) {
        self.insert_frame(vaddr, new_frame)
            .expect("Frame not exist");
    }
}

impl<B: MappingBackend> fmt::Debug for MemorySet<B>
where
    B::Addr: fmt::Debug,
    B::Flags: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_list().entries(self.areas.values()).finish()
    }
}
