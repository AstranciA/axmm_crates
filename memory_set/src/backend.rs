use alloc::collections::BTreeMap;
use alloc::string::ToString;
use core::ops::Deref;

use memory_addr::MemoryAddr;

/// Underlying operations to do when manipulating mappings within the specific
/// [`MemoryArea`](crate::MemoryArea).
///
/// The backend can be different for different memory areas. e.g., for linear
/// mappings, the target physical address is known when it is added to the page
/// table. For lazy mappings, an empty mapping needs to be added to the page
/// table to trigger a page fault.
pub trait MappingBackend: Clone {
    /// The address type used in the memory area.
    type Addr: MemoryAddr;
    /// The flags type used in the memory area.
    type Flags: Copy + ToString;
    /// The page table type used in the memory area.
    type PageTable;

    #[cfg(feature = "RAII")]
    type FrameTrackerImpl: memory_addr::FrameTracker;
    #[cfg(feature = "RAII")]
    type FrameTrackerRef: Deref<Target = Self::FrameTrackerImpl> + Clone;

    #[cfg(feature = "RAII")]
    /// What to do when mapping a region within the area with the given flags.
    fn map(
        &self,
        start: Self::Addr,
        size: usize,
        flags: Self::Flags,
        page_table: &mut Self::PageTable,
    ) -> Result<BTreeMap<Self::Addr, Self::FrameTrackerRef>, ()>;

    #[cfg(not(feature = "RAII"))]
    /// What to do when mapping a region within the area with the given flags.
    fn map(
        &self,
        start: Self::Addr,
        size: usize,
        flags: Self::Flags,
        page_table: &mut Self::PageTable,
    ) -> Result<(), ()>;

    /// What to do when unmaping a memory region within the area.
    /// Should not deallocate frames if RAII is on.
    fn unmap(&self, start: Self::Addr, size: usize, page_table: &mut Self::PageTable) -> bool;

    /// What to do when changing access flags.
    fn protect(
        &self,
        start: Self::Addr,
        size: usize,
        new_flags: Self::Flags,
        page_table: &mut Self::PageTable,
    ) -> bool;
}
