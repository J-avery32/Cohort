use crate::util::Aligned;
use core::ptr::NonNull;
use std::{
    alloc::{alloc, dealloc, Layout},
    cell::UnsafeCell,
    mem, ptr,
};

#[repr(packed)]
pub struct Meta<T> {
    buffer: NonNull<T>,
    _elem_size: u32,
    capacity: u32,
}

#[repr(C)]
pub struct CohortFifo<T: Copy> {
    // Cohort requires that these fields be 128 byte alligned and in the specified order.
    head: Aligned<UnsafeCell<u32>>,
    meta: Aligned<Meta<T>>,
    tail: Aligned<UnsafeCell<u32>>,
}

impl<T: Copy> CohortFifo<T> {
    // Creates new fifo.
    pub fn new(capacity: usize) -> Self {
        let buffer = unsafe {
            let buffer_size = capacity + 1;
            let layout = Layout::array::<T>(buffer_size).unwrap();
            let aligned = layout.align_to(128).unwrap();
            NonNull::new(alloc(aligned)).unwrap()
        };

        CohortFifo {
            head: Aligned(UnsafeCell::new(0)),
            meta: Aligned(Meta {
                buffer: buffer.cast(),
                _elem_size: mem::size_of::<T>() as u32,
                capacity: capacity as u32,
            }),
            tail: Aligned(UnsafeCell::new(0)),
        }
    }

    pub fn try_push(&self, elem: T) -> Result<(), T> {
        if self.is_full() {
            return Err(elem);
        }

        let tail = self.tail();
        unsafe {
            (*self.buffer().as_ptr())[tail] = elem;
        }
        self.set_tail((tail + 1) % self.buffer_size());

        Ok(())
    }

    /// Pushes an element to the fifo.
    pub fn push(&self, elem: T) {
        while self.try_push(elem).is_err() {}
    }

    pub fn try_pop(&self) -> Result<T, ()> {
        if self.is_empty() {
            return Err(());
        }

        let head = self.head();
        let elem = unsafe { (*self.buffer().as_ptr())[head] };
        self.set_head((head + 1) % self.buffer_size());

        Ok(elem)
    }

    /// Pops an element from the fifo.
    pub fn pop(&self) -> T {
        loop {
            if let Ok(data) = self.try_pop() {
                break data;
            }
        }
    }

    // pub fn capacity(&self) -> usize {
    //     self.meta.0.capacity as usize
    // }

    /// True size of the underlying buffer.
    fn buffer_size(&self) -> usize {
        // Should always be one more than the given capacity.
        // The extra allocated slot in the buffer is used to determine whether the buffer is full.
        (self.meta.0.capacity + 1) as usize
    }

    pub fn is_full(&self) -> bool {
        (self.head() % self.buffer_size()) == ((self.tail() + 1) % self.buffer_size())
    }

    pub fn is_empty(&self) -> bool {
        self.head() == self.tail()
    }

    fn head(&self) -> usize {
        unsafe { ptr::read_volatile(self.head.0.get()) as usize }
    }

    fn tail(&self) -> usize {
        unsafe { ptr::read_volatile(self.tail.0.get()) as usize }
    }

    fn set_head(&self, head: usize) {
        unsafe {
            ptr::write_volatile(self.head.0.get(), head as u32);
        }
    }

    fn set_tail(&self, tail: usize) {
        unsafe {
            ptr::write_volatile(self.tail.0.get(), tail as u32);
        }
    }

    fn buffer(&self) -> NonNull<[T]> {
        NonNull::slice_from_raw_parts(self.meta.0.buffer, self.buffer_size())
    }
}

unsafe impl<T: Copy> Send for CohortFifo<T> {}
unsafe impl<T: Copy> Sync for CohortFifo<T>{}

impl<T: Copy> Drop for CohortFifo<T> {
    fn drop(&mut self) {
        let layout = Layout::array::<T>(self.buffer_size()).unwrap();
        let aligned = layout.align_to(128).unwrap();
        unsafe { dealloc(self.meta.0.buffer.cast().as_ptr(), aligned) };
    }
}

#[cfg(test)]
mod tests {
    use std::thread;

    use super::CohortFifo;

    #[test]
    fn initializes_empty() {
        let spsc = CohortFifo::<u32>::new(10);
        assert!(spsc.is_empty());
    }

    #[test]
    fn test_filling_up_and_test_extra_push_and_test_emptying_and_test_extra_pop(){
        let spsc = CohortFifo::<u32>::new(10);
        for n in 0..10 {
            spsc.push(n);
        }
        assert!(spsc.try_push(11).is_err());
        assert!(spsc.is_full());
        for n in 0..10 {
            assert!(spsc.pop() == n);
        }
        assert!(spsc.is_empty());
        assert!(spsc.try_pop().is_err());
    }

    #[test]
    fn test_two_threads(){
        let spsc = CohortFifo::<u32>::new(10);

        thread::scope( |s| {
            const THROUGHPUT: u32 = 1_000;
            let handle = s.spawn(|| {
            for i in 0..THROUGHPUT {
                spsc.push(i);
            }
        });

        for i in 0..THROUGHPUT {
            let elem = spsc.pop();
            assert!(elem==i);
        }
        assert!(spsc.is_empty());
       
    });

    }
}