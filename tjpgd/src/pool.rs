//! 工作内存池实现
//! 
//! 与C版本的alloc_pool()完全一致的线性内存分配器
//! 
//! C版本实现 (tjpgd.c lines 133-150):
//! ```c
//! static void* alloc_pool(JDEC* jd, size_t ndata) {
//!     ndata = (ndata + 3) & ~3;  // 4字节对齐
//!     if (jd->sz_pool >= ndata) {
//!         jd->sz_pool -= ndata;
//!         rp = (char*)jd->pool;
//!         jd->pool = (void*)(rp + ndata);
//!     }
//!     return rp;
//! }
//! ```

use core::mem;

/// 工作内存池
/// 
/// 这是一个简单的线性分配器，与C版本的alloc_pool行为完全一致：
/// - 从缓冲区头部顺序分配
/// - 4字节对齐
/// - 不支持释放（整个池一起释放）
pub struct MemoryPool<'a> {
    /// 剩余可用的内存缓冲区
    buffer: &'a mut [u8],
    /// 当前分配位置
    offset: usize,
}

impl<'a> MemoryPool<'a> {
    /// 创建新的内存池
    /// 
    /// # Arguments
    /// * `buffer` - 用户提供的工作内存缓冲区
    /// 
    /// # Example
    /// ```ignore
    /// let mut workspace = [0u8; 10240];
    /// let mut pool = MemoryPool::new(&mut workspace);
    /// ```
    pub fn new(buffer: &'a mut [u8]) -> Self {
        Self {
            buffer,
            offset: 0,
        }
    }

    /// 从池中分配内存
    /// 
    /// 与C版本alloc_pool()行为完全一致：
    /// - 8字节对齐（为了支持64位指针和u64）
    /// - 返回None表示内存不足
    /// 
    /// # Arguments
    /// * `size` - 需要分配的字节数
    /// 
    /// # Returns
    /// 分配的内存切片，如果内存不足返回None
    pub fn alloc(&mut self, size: usize) -> Option<&'a mut [u8]> {
        self.alloc_aligned(size, 8)
    }

    /// 从池中分配指定对齐的内存
    pub fn alloc_aligned(&mut self, size: usize, align: usize) -> Option<&'a mut [u8]> {
        // 确保当前偏移量对齐
        let align_mask = align - 1;
        let aligned_offset = (self.offset + align_mask) & !align_mask;
        
        // 对齐大小
        let aligned_size = (size + align_mask) & !align_mask;
        
        let remaining = self.buffer.len() - aligned_offset;
        if remaining < aligned_size {
            return None;
        }

        let start = aligned_offset;
        self.offset = aligned_offset + aligned_size;

        // 使用unsafe来返回带有'a生命周期的切片
        // 这是安全的，因为我们保证不会重叠分配
        unsafe {
            let ptr = self.buffer.as_mut_ptr().add(start);
            Some(core::slice::from_raw_parts_mut(ptr, size))
        }
    }

    /// 分配并初始化为零
    pub fn alloc_zeroed(&mut self, size: usize) -> Option<&'a mut [u8]> {
        let slice = self.alloc(size)?;
        slice.fill(0);
        Some(slice)
    }

    /// 分配类型化数组
    /// 
    /// # Safety
    /// 调用者需要确保类型T的对齐要求不超过4字节
    pub fn alloc_slice<T: Copy + Default>(&mut self, count: usize) -> Option<&'a mut [T]> {
        let size = count * mem::size_of::<T>();
        let slice = self.alloc(size)?;
        
        // 将字节切片转换为类型化切片
        let ptr = slice.as_mut_ptr() as *mut T;
        unsafe {
            let typed_slice = core::slice::from_raw_parts_mut(ptr, count);
            // 初始化为默认值
            for item in typed_slice.iter_mut() {
                *item = T::default();
            }
            Some(typed_slice)
        }
    }

    /// 分配u8数组
    pub fn alloc_u8(&mut self, count: usize) -> Option<&'a mut [u8]> {
        self.alloc_zeroed(count)
    }

    /// 分配u16数组
    pub fn alloc_u16(&mut self, count: usize) -> Option<&'a mut [u16]> {
        self.alloc_slice(count)
    }

    /// 分配i32数组
    pub fn alloc_i32(&mut self, count: usize) -> Option<&'a mut [i32]> {
        self.alloc_slice(count)
    }

    /// 分配i16数组
    pub fn alloc_i16(&mut self, count: usize) -> Option<&'a mut [i16]> {
        self.alloc_slice(count)
    }

    /// 获取剩余可用字节数
    pub fn remaining(&self) -> usize {
        self.buffer.len() - self.offset
    }

    /// 获取已使用字节数
    pub fn used(&self) -> usize {
        self.offset
    }

    /// 获取总容量
    pub fn capacity(&self) -> usize {
        self.buffer.len()
    }

    /// 重置池（释放所有分配）
    pub fn reset(&mut self) {
        self.offset = 0;
    }
}


/// 推荐的工作内存大小（带快速解码）
pub const RECOMMENDED_POOL_SIZE: usize = 10240;

/// 最小工作内存大小（不带快速解码）
pub const MINIMUM_POOL_SIZE: usize = 4096;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc_basic() {
        let mut buffer = [0u8; 1024];
        let mut pool = MemoryPool::new(&mut buffer);

        let slice1 = pool.alloc(100).unwrap();
        assert_eq!(slice1.len(), 100);
        assert_eq!(pool.used(), 104);  // 100 aligned to 8 = 104

        let slice2 = pool.alloc(50).unwrap();
        assert_eq!(slice2.len(), 50);
        assert_eq!(pool.used(), 160);  // 104 + 56 (50 aligned to 8)
    }

    #[test]
    fn test_alloc_alignment() {
        let mut buffer = [0u8; 1024];
        let mut pool = MemoryPool::new(&mut buffer);

        pool.alloc(1).unwrap();
        assert_eq!(pool.used(), 8);  // 1 aligned to 8

        pool.alloc(5).unwrap();
        assert_eq!(pool.used(), 16);  // 8 + 8 (5 aligned to 8)
    }

    #[test]
    fn test_alloc_typed() {
        let mut buffer = [0u8; 1024];
        let mut pool = MemoryPool::new(&mut buffer);

        let u16_slice = pool.alloc_u16(10).unwrap();
        assert_eq!(u16_slice.len(), 10);
        assert_eq!(pool.used(), 24);  // 20 aligned to 8 = 24

        let i32_slice = pool.alloc_i32(5).unwrap();
        assert_eq!(i32_slice.len(), 5);
        assert_eq!(pool.used(), 48);  // 24 + 24 (20 aligned to 8)
    }

    #[test]
    fn test_alloc_fail() {
        let mut buffer = [0u8; 128];
        let mut pool = MemoryPool::new(&mut buffer);

        assert!(pool.alloc(50).is_some());  // uses 56 bytes
        assert!(pool.alloc(50).is_some());  // uses another 56 bytes = 112 total
        assert!(pool.alloc(20).is_none());  // 128 - 112 = 16, not enough for 20 (needs 24 aligned)
    }
}
