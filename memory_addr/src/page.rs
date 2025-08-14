//! 支持混合存储不同尺寸内存页的内存管理系统

use crate::PhysAddr;

/// 页帧追踪器
pub trait FrameTracker {
    const PAGE_SIZE: usize;
    /*
     * /// 创建新页（需保证地址对齐）
     * fn new(va: VirtAddr) -> Self;
     */

    /// new FrameTracker without alloc
    fn new(pa: PhysAddr) -> Self;

    /// new FrameTracker without alloc and dealloc
    fn no_tracking(pa: PhysAddr) -> Self;

    /// new FrameTracker with alloc
    fn alloc_frame() -> Self;

    fn dealloc_frame(&mut self);

    /// 获取起始地址
    fn start(&self) -> PhysAddr;

    /// 获取页大小
    /// size is a const generic parameter
    fn size(&self) -> usize {
        Self::PAGE_SIZE
    }

    fn as_ptr(&self) -> *const u8 {
        self.start().as_usize() as *const u8
    }

    fn as_mut_ptr(&mut self) -> *mut u8 {
        self.start().as_usize() as *mut u8
    }

    /// 获取不可变数据切片
    fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.as_ptr(), Self::PAGE_SIZE) }
    }

    /// 获取可变数据切片
    fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.as_mut_ptr(), Self::PAGE_SIZE) }
    }
}

pub trait Page: FrameTracker {}

// 动态页接口（类型擦除用）
/*
 *pub trait DynamicPage: Send + Sync {
 *    /// 获取页大小
 *    fn size(&self) -> usize;
 *
 *    /// 获取起始地址
 *    fn start(&self) -> VirtAddr;
 *
 *    fn as_ptr(&self) -> *const u8 {
 *        self.start().as_usize() as *const u8
 *    }
 *
 *    fn as_mut_ptr(&mut self) -> *mut u8 {
 *        self.start().as_usize() as *mut u8
 *    }
 *
 *    /// 获取不可变数据切片
 *    fn as_slice(&self) -> &[u8]{
 *        unsafe { core::slice::from_raw_parts(self.as_ptr(), self.size()) }
 *    }
 *
 *    /// 获取可变数据切片
 *    fn as_mut_slice(&mut self) -> &mut [u8]{
 *        unsafe{core::slice::from_raw_parts_mut(self.as_mut_ptr(),
 * self.size())}    }
 *
 *    /// 全页写入
 *    fn write(&mut self, data: &[u8]) {
 *        assert_eq!(data.len(), self.size());
 *        self.as_mut_slice().copy_from_slice(data);
 *    }
 *
 *    /// 带偏移量写入
 *    fn write_at(&mut self, data: &[u8], offset: usize) -> bool {
 *        if offset + data.len() > self.size() {
 *            return false;
 *        }
 *        self.as_mut_slice()[offset..offset +
 * data.len()].copy_from_slice(data);        true
 *    }
 *}
 */

///// 动态页转换中间层
//pub trait IntoDynamicPage {
///// 转换为动态页对象
//fn into_dyn(self) -> Box<dyn DynamicPage>;
//}

// 页结构体封装
/*
 *pub struct Page<T>
 *where
 *    T: dyn FrameTracker,
 *{
 *    inner: impl FrameTracker,
 *}
 *
 *impl<T, const PAGE_SIZE: usize> Page<T, PAGE_SIZE>
 *where
 *    T: FrameTracker<PAGE_SIZE> + 'static,
 *{
 *    /// 分配物理页
 *    pub fn alloc(va: VirtAddr) -> Self {
 *        assert!(va.is_aligned(PAGE_SIZE), "address not aligned");
 *        Self {
 *            inner: T::alloc_frame(),
 *        }
 *    }
 *}
 *
 * // 实现自动解引用
 *impl<T, const PAGE_SIZE: usize> Deref for Page<T, PAGE_SIZE>
 *where
 *    T: FrameTracker<PAGE_SIZE>,
 *{
 *    type Target = T;
 *
 *    fn deref(&self) -> &Self::Target {
 *        &self.inner
 *    }
 *}
 *
 *impl<T, const PAGE_SIZE: usize> DerefMut for Page<T, PAGE_SIZE>
 *where
 *    T: FrameTracker<PAGE_SIZE>,
 *{
 *    fn deref_mut(&mut self) -> &mut Self::Target {
 *        &mut self.inner
 *    }
 *}
 */

// 实现动态页转换
//impl<T, const PAGE_SIZE: usize> IntoDynamicPage for Page<T, PAGE_SIZE>
//where
//T: FrameTracker<PAGE_SIZE> + 'static,
//{
//fn into_dyn(self) -> Box<dyn DynamicPage> {
//Box::new(PageWrapper {
//inner: self.inner,
//_marker: PhantomData,
//})
//}

/*
 * // 私有包装结构实现DynamicPage
 *struct PageWrapper<T, const PAGE_SIZE: usize> {
 *    inner: T,
 *    _marker: PhantomData<T>, // 使用类型关联的PhantomData
 *}
 *
 * // 手动实现Send/Sync
 *unsafe impl<T, const PAGE_SIZE: usize> Send for PageWrapper<T, PAGE_SIZE>
 * where T: Send {} unsafe impl<T, const PAGE_SIZE: usize> Sync for
 * PageWrapper<T, PAGE_SIZE> where T: Sync {}
 *
 *impl<T, const PAGE_SIZE: usize> DynamicPage for PageWrapper<T, PAGE_SIZE>
 *where
 *    T: FrameTracker<PAGE_SIZE> + Send + Sync,
 *{
 *    fn size(&self) -> usize {
 *        PAGE_SIZE
 *    }
 *
 *    fn start(&self) -> VirtAddr {
 *        self.inner.start()
 *    }
 *}
 */
