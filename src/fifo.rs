use crate::error::Error;
use crate::{util::Aligned, Result};
use core::ptr::NonNull;
use std::sync::atomic::{fence, Ordering};
use std::{
    alloc::{alloc_zeroed, dealloc, Layout},
    cell::UnsafeCell,
    mem, ptr,
};

#[repr(packed)]
pub struct Meta<T> {
    buffer: NonNull<T>,
    _elem_size: u32,
    buffer_size: u32,
}

#[repr(C)]
pub struct CohortFifo<T: Copy + std::fmt::Debug> {
    // Cohort requires that these fields be 128 byte alligned and in the specified order.
    head: Aligned<UnsafeCell<u32>>,
    meta: Aligned<Meta<T>>,
    hw_tail: Aligned<UnsafeCell<u32>>,

    
    //Extra fields not used by cohort accelerators
    // This determines the number of elements that can be pushed to the queue
    // before we increment the hw_tail
    batch_size: usize,
    // This is the tail used internally by the software to keep track of the
    // true number of elements pushed to the queue
    sw_tail: Aligned<UnsafeCell<u32>>,
}

/// An iterator over the elements currently in the FIFO queue.
pub struct CohortFifoIter<'a, T: Copy + std::fmt::Debug> {
    fifo: &'a CohortFifo<T>,
    idx: usize,
    remaining: usize,
}

impl<'a, T: Copy + std::fmt::Debug> Iterator for CohortFifoIter<'a, T> {
    type Item = (usize, T);
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let buffer = unsafe { self.fifo.buffer().as_ref() };
        let idx = self.idx;
        let value = buffer[idx];
        self.idx = (self.idx + 1) % self.fifo.buffer_size();
        self.remaining -= 1;
        Some((idx, value))
    }
}

impl<'a, T: Copy + std::fmt::Debug> ExactSizeIterator for CohortFifoIter<'a, T> {
    fn len(&self) -> usize {
        self.remaining
    }
}

/// An iterator over the elements currently in the FIFO queue.
pub struct CohortFifoIter<'a, T: Copy + std::fmt::Debug> {
    fifo: &'a CohortFifo<T>,
    idx: usize,
    remaining: usize,
}

impl<'a, T: Copy + std::fmt::Debug> Iterator for CohortFifoIter<'a, T> {
    type Item = (usize, T);
    fn next(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        let buffer = unsafe { self.fifo.buffer().as_ref() };
        let idx = self.idx;
        let value = buffer[idx];
        self.idx = (self.idx + 1) % self.fifo.buffer_size();
        self.remaining -= 1;
        Some((idx, value))
    }
}

impl<'a, T: Copy + std::fmt::Debug> ExactSizeIterator for CohortFifoIter<'a, T> {
    fn len(&self) -> usize {
        self.remaining
    }}

impl<T: Copy + std::fmt::Debug> CohortFifo<T> {
    // Creates new fifo.
    pub fn new(capacity: usize, batch_size: usize) -> Result<Self> {
        if (batch_size < 2){
            return Err(Error::BatchSizeTooSmall);
        }

        if (batch_size % 2 != 0){
            return Err(Error::BatchSizeNotEven);
        }

        if(capacity < batch_size) {
            return Err(Error::CapacityLessThanBatchSize);
        }
        // Capacity must
        if(capacity % 2 != 0){
            return Err(Error::CapacityNotEven);
        }
        let buffer = unsafe {
            let buffer_size = capacity + 1;
            let layout = Layout::from_size_align(buffer_size * mem::size_of::<T>(), 128).unwrap();
            NonNull::new(alloc_zeroed(layout)).unwrap()
        };

        Ok(CohortFifo {
            head: Aligned(UnsafeCell::new(0)),
            meta: Aligned(Meta {
                buffer: buffer.cast(),
                _elem_size: mem::size_of::<T>() as u32,
                buffer_size: (capacity + 1) as u32,
            }),
            hw_tail: Aligned(UnsafeCell::new(0)),


            batch_size,
            sw_tail: Aligned(UnsafeCell::new(0)),
        })
    }

    pub fn try_push(&self, elem1: &T, elem2: &T) -> Result<()> {
        if self.is_full() {
            return Err(Error::Full);
        }
        // println!("-----SENDER QUEUE------");
        // self.print_queue();
        let sw_tail = self.sw_tail();
        unsafe {
            (*self.buffer().as_ptr())[sw_tail] = *elem1;
            (*self.buffer().as_ptr())[(sw_tail + 1) % self.buffer_size()] = *elem2;
        }

        self.set_tail((tail + 2) % self.buffer_size());
        // println!("Tail advanced to: {:?}", self.tail());
        Ok(())
    }

    /// Pushes an element to the fifo.
    pub fn push(&self, elem1: &T, elem2: &T) {
        while self.try_push(elem1, elem2).is_err() {}
    }

    pub fn try_pop(&self, elem1: &mut T, elem2: &mut T) -> Result<(), ()> {
        // Ensure that the accelerator has pushed at least two elements onto the queue
        if self.is_empty() || self.num_elems() == 1 {
            // println!("NUMBER OF ELEMS: {}", self.num_elems());
            return Err(Error::Empty);
        }
        // println!("---------RECEIVER QUEUE--------");
        // self.print_queue();
        let head = self.head();
        *elem1 = unsafe { (*self.buffer().as_ptr())[head] };
        *elem2 = unsafe { (*self.buffer().as_ptr())[(head + 1) % self.buffer_size()] };

        self.set_head((head + 2) % self.buffer_size());
        // println!("Head advanced to: {:?}", self.head());
        Ok(())
    }

    /// Pops an element from the fifo.
    pub fn pop(&self, elem1: &mut T, elem2: &mut T) {
        loop {
            if let Ok(()) = self.try_pop(elem1, elem2) {
                break;
            }
        }
    }

    /// Returns the true size of the underlying buffer (capacity + 1).
    fn buffer_size(&self) -> usize {
        // Should always be one more than the given capacity.
        // The extra allocated slot in the buffer is used to determine whether the buffer is full.
        (self.meta.0.buffer_size) as usize
    }

    /// TODO: BIG PROBLEM HERE!!!!! is_full() uses the sw_tail and so 
    /// if we are a receiver queue and we use this without updating the 
    /// sw_tail to the hw_tail set by the accelerator this function is inaccurate.
    /// 
    /// Currently we fix this by updating the hw_tail in try_pop before we call these
    /// functions. But there must be a more elegant way to fix this...
    fn is_full(&self) -> bool {
        (self.head() % self.buffer_size()) == ((self.sw_tail() + 1) % self.buffer_size())
    }

    /// TODO: BIG PROBLEM HERE!!!! SEE ABOVE COMMENT!!!!!
    fn is_empty(&self) -> bool {
        self.head() == self.sw_tail()
    }

    /// TODO: BIG PROBLEM HERE!!!! SEE ABOVE COMMENT!!!!!
    fn num_elems(&self) -> usize {
        if self.head() >= self.sw_tail() {
            return (self.head()-self.sw_tail()); 
        } else {
            return self.capacity() + self.head() - self.sw_tail();
        }
    }

    fn head(&self) -> usize {
        unsafe { ptr::read_volatile(self.head.0.get()) as usize }
    }

    /// Returns the current software tail index.
    fn sw_tail(&self) -> usize {
        unsafe { ptr::read_volatile(self.sw_tail.0.get()) as usize }
    }

    /// Returns the current hardware tail index.
    fn hw_tail(&self) -> usize {
        unsafe { ptr::read_volatile(self.hw_tail.0.get()) as usize }
    }

    /// Sets the head index to the given value with memory ordering guarantees.
    fn set_head(&self, head: usize) {
        fence(Ordering::SeqCst);
        unsafe {
            ptr::write_volatile(self.head.0.get(), head as u32);
        }
        fence(Ordering::SeqCst);

    }

    fn set_hw_tail(&self, tail: usize) {
        fence(Ordering::SeqCst);
        unsafe {
            ptr::write_volatile(self.hw_tail.0.get(), tail as u32);
        }
        fence(Ordering::SeqCst);

    }

    fn set_sw_tail(&self, tail: usize) {
        fence(Ordering::SeqCst);
        unsafe {
            ptr::write_volatile(self.tail.0.get(), tail as u32);
        }
        fence(Ordering::SeqCst);

    }

    fn set_sw_tail(&self, tail: usize) {
        fence(Ordering::SeqCst);
        unsafe {
            ptr::write_volatile(self.sw_tail.0.get(), tail as u32);
        }
        fence(Ordering::SeqCst);
    }

    /// Returns a non-null pointer to the buffer as a slice.
    fn buffer(&self) -> NonNull<[T]> {
        NonNull::slice_from_raw_parts(self.meta.0.buffer, self.buffer_size())
    }


    fn capacity(&self) -> usize {
        self.buffer_size()-1
    }
}

unsafe impl<T: Copy + std::fmt::Debug> Send for CohortFifo<T> {}
unsafe impl<T: Copy + std::fmt::Debug> Sync for CohortFifo<T> {}

impl<T: Copy + std::fmt::Debug> Drop for CohortFifo<T> {
    fn drop(&mut self) {
        let layout = Layout::array::<T>(self.buffer_size()).unwrap();
        let aligned = layout.align_to(128).unwrap();
        unsafe { dealloc(self.meta.0.buffer.cast().as_ptr(), aligned) };
    }
}

#[cfg(test)]
mod tests {
    use super::CohortFifo;

    #[test]
    fn initializes_empty() {
        let spsc = CohortFifo::<[u8; 16]>::new(10).unwrap();
        assert!(spsc.is_empty());
    }

    #[test]
    fn test_fifo_fill_and_full() {
        // Create a FIFO with capacity for 10 elements (5 pairs)
        let spsc = CohortFifo::<[u8; 16]>::new(10).unwrap();
        // Fill the queue with 5 pairs of elements
        for n in 0..5 {
            // Each pair is (n*2, n*2+1) for easy verification
            let val1: [u8; 16] = [n * 2; 16];
            let val2: [u8; 16] = [n * 2 + 1; 16];
            assert!(spsc.try_push(&val1, &val2).is_ok());
        }
        // Debug print the FIFO state
        println!("{spsc}");
        // The FIFO should now be full
        assert!(spsc.is_full());
    }

    #[test]
    fn test_fifo_extra_push_when_full() {
        // Fill the FIFO to capacity
        let spsc = CohortFifo::<[u8; 16]>::new(10).unwrap();
        for n in 0..5 {
            let val1: [u8; 16] = [n * 2; 16];
            let val2: [u8; 16] = [n * 2 + 1; 16];
            assert!(spsc.try_push(&val1, &val2).is_ok());
        }
        // Confirm the FIFO is full
        assert!(spsc.is_full());
        // Try to push another pair, which should fail
        assert!(spsc.try_push(&[11; 16], &[12; 16]).is_err());
        // The FIFO should still be full
        assert!(spsc.is_full());
    }

    #[test]
    fn test_fifo_emptying() {
        // Fill the FIFO with 5 pairs
        let spsc = CohortFifo::<[u8; 16]>::new(10).unwrap();
        for n in 0..5 {
            let val1: [u8; 16] = [n * 2; 16];
            let val2: [u8; 16] = [n * 2 + 1; 16];
            assert!(spsc.try_push(&val1, &val2).is_ok());
        }
        // Pop all 5 pairs and check their values
        for n in 0..5 {
            let mut val1 = [0; 16];
            let mut val2 = [0; 16];
            assert!(spsc.try_pop(&mut val1, &mut val2).is_ok());
            // Each pop should return the expected pair
            assert_eq!(val1, [n * 2; 16]);
            assert_eq!(val2, [n * 2 + 1; 16]);
        }
        // The FIFO should now be empty
        assert!(spsc.is_empty());
    }

    #[test]
    fn test_fifo_refill_and_empty_again() {
        // Create and fill the FIFO
        let spsc = CohortFifo::<[u8; 16]>::new(10).unwrap();
        println!("New FIFO: {spsc}");

        println!("Filling FIFO");
        for n in 0..5 {
            println!("Fifo (n={n}): {spsc}");
            let val1: [u8; 16] = [n * 2; 16];
            let val2: [u8; 16] = [n * 2 + 1; 16];
            assert!(spsc.try_push(&val1, &val2).is_ok());
        }
        // Empty the FIFO completely
        println!("Emptying FIFO");
        for n in 0..5 {
            println!("Fifo (n={n}): {spsc}");
            let mut val1 = [0; 16];
            let mut val2 = [0; 16];
            assert!(spsc.try_pop(&mut val1, &mut val2).is_ok());
        }
        // Refill the FIFO with the same pattern
        println!("Refilling FIFO");
        for n in 0..5 {
            println!("Fifo (n={n}): {spsc}");
            let val1: [u8; 16] = [n * 2; 16];
            let val2: [u8; 16] = [n * 2 + 1; 16];
            assert!(spsc.try_push(&val1, &val2).is_ok());
            println!("Fifo: {}", spsc);
        }
        // Empty again and check values
        println!("Reemptying FIFO");
        for n in 0..5 {
            println!("Fifo (n={n}): {spsc}");
            let mut val1 = [0; 16];
            let mut val2 = [0; 16];
            assert!(spsc.try_pop(&mut val1, &mut val2).is_ok());
            assert_eq!(val1, [n * 2; 16]);
            assert_eq!(val2, [n * 2 + 1; 16]);
        }
        // The FIFO should be empty again
        assert!(spsc.is_empty());
    }

    #[test]
    fn test_fifo_try_pop_when_empty() {
        // Create an empty FIFO
        let spsc = CohortFifo::<[u8; 16]>::new(10).unwrap();
        let mut val1 = [0; 16];
        let mut val2 = [0; 16];
        // Try to pop from the empty FIFO, which should fail
        assert!(spsc.try_pop(&mut val1, &mut val2).is_err());
    }

    #[test]
    fn test_two_threads() {
        let spsc = CohortFifo::<[u8; 16]>::new(10).unwrap();

        std::thread::scope(|s| {
            const THROUGHPUT: u32 = 10_000_000;
            let handle = s.spawn(|| {
                for i in 0..THROUGHPUT / 2 {
                    let v1 = [(i % 64) as u8; 16];
                    let v2 = [((i + 1) % 64) as u8; 16];
                    assert!(spsc.try_push(&v1, &v2).is_ok());
                }
            });

            for i in 0..THROUGHPUT / 2 {
                let mut elem1 = [0; 16];
                let mut elem2 = [0; 16];
                assert!(spsc.try_pop(&mut elem1, &mut elem2).is_ok());
                assert_eq!(elem1, [(i % 64) as u8; 16]);
                assert_eq!(elem2, [((i + 1) % 64) as u8; 16]);
            }
            assert!(spsc.is_empty());
            handle.join().unwrap();
        });
    }

    #[test]
    fn wraparound_behavior() {
        // Test that the FIFO correctly wraps around the buffer boundary.
        let spsc = CohortFifo::<u8>::new(4).unwrap();
        // Fill the buffer (capacity is 4, so 4 elements)
        assert!(spsc.try_push(&1, &2).is_ok());
        assert!(spsc.try_push(&3, &4).is_ok());
        assert!(spsc.is_full());
        // Pop two elements
        let mut a = 0;
        let mut b = 0;
        assert!(spsc.try_pop(&mut a, &mut b).is_ok());
        assert_eq!((a, b), (1, 2));
        // Push two more, should wrap around
        assert!(spsc.try_push(&5, &6).is_ok());
        assert!(spsc.is_full());
        // Pop all remaining
        assert!(spsc.try_pop(&mut a, &mut b).is_ok());
        assert_eq!((a, b), (3, 4));
        assert!(spsc.try_pop(&mut a, &mut b).is_ok());
        assert_eq!((a, b), (5, 6));
        assert!(spsc.is_empty());
    }

    #[test]
    fn never_overflow_or_underflow() {
        // Test that the FIFO never overflows or underflows, even with repeated wraparounds.
        let spsc = CohortFifo::<u32>::new(8).unwrap();
        let mut expected = 0u32;
        for _ in 0..100 {
            // Fill
            for i in 0..4 {
                assert!(spsc
                    .try_push(&(expected + i * 2), &(expected + i * 2 + 1))
                    .is_ok());
            }
            assert!(spsc.is_full());
            // Empty
            for i in 0..4 {
                let mut a = 0;
                let mut b = 0;
                assert!(spsc.try_pop(&mut a, &mut b).is_ok());
                assert_eq!(a, expected + i * 2);
                assert_eq!(b, expected + i * 2 + 1);
            }
            assert!(spsc.is_empty());
            expected += 8;
        }
    }

    #[test]
    fn edge_case_full_empty() {
        // Test that the FIFO correctly handles transitions between full and empty.
        let spsc = CohortFifo::<u8>::new(2).unwrap();
        assert!(spsc.is_empty());
        assert!(spsc.try_push(&1, &2).is_ok());
        assert!(spsc.is_full());
        let mut a = 0;
        let mut b = 0;
        assert!(spsc.try_pop(&mut a, &mut b).is_ok());
        assert_eq!((a, b), (1, 2));
        assert!(spsc.is_empty());
        // Try popping again, should fail
        assert!(spsc.try_pop(&mut a, &mut b).is_err());
        // Try pushing again
        assert!(spsc.try_push(&3, &4).is_ok());
        assert!(spsc.is_full());
        assert!(spsc.try_pop(&mut a, &mut b).is_ok());
        assert_eq!((a, b), (3, 4));
        assert!(spsc.is_empty());
    }
}
