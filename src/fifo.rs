use crate::error::Error;
use crate::{Result, util::Aligned};
use core::ptr::NonNull;
use std::sync::atomic::{fence, Ordering};
use std::{
    alloc::{alloc, dealloc, Layout},
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
    tail: Aligned<UnsafeCell<u32>>,
}

impl<T: Copy + std::fmt::Debug> CohortFifo<T> {
    // Creates new fifo.
    pub fn new(capacity: usize) -> Result<Self> {
        // Capacity must be divisible by 2.
        if capacity % 2 != 0 {
            return Err(Error::Capacity(capacity));
        }
        let buffer = unsafe {
            let buffer_size = capacity + 1;
            let layout = Layout::array::<T>(buffer_size).unwrap();
            let aligned = layout.align_to(128).unwrap();
            NonNull::new(alloc(aligned)).unwrap()
        };

        Ok(CohortFifo {
            head: Aligned(UnsafeCell::new(0)),
            meta: Aligned(Meta {
                buffer: buffer.cast(),
                _elem_size: mem::size_of::<T>() as u32,
                buffer_size: (capacity + 1) as u32,
            }),
            tail: Aligned(UnsafeCell::new(0)),
        })
    }

    pub fn try_push(&self, elem1: &T, elem2: &T) -> Result<()> {
        if self.is_full() {
            return Err(Error::Full);
        }
        // println!("-----SENDER QUEUE------");
        // self.print_queue();
        let tail = self.tail();
        unsafe {
            (*self.buffer().as_ptr())[tail] = *elem1;
            (*self.buffer().as_ptr())[(tail + 1) % self.buffer_size()] = *elem2;
        }

        self.set_tail((tail + 2) % self.buffer_size());
        // println!("Tail advanced to: {:?}", self.tail());
        Ok(())
    }

    /// Pushes an element to the fifo.
    pub fn push(&self, elem1: &T, elem2: &T) {
        while self.try_push(elem1, elem2).is_err() {}
    }

    pub fn try_pop(&self, elem1: &mut T, elem2: &mut T) -> Result<()> {
        // Ensure that the accelerator has pushed at least two elements onto the queue
        if self.is_empty() || self.num_elems() == 1 {
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

    /// Prints the contents of the underlying buffer for debugging.
    pub fn print_queue(&self) {
        unsafe { println!("{:?}", self.buffer().as_ref()) };
    }

    /// Returns the true size of the underlying buffer (capacity + 1).
    fn buffer_size(&self) -> usize {
        // Should always be one more than the given capacity.
        // The extra allocated slot in the buffer is used to determine whether the buffer is full.
        (self.meta.0.buffer_size) as usize
    }

    /// Returns true if the FIFO is full.
    pub fn is_full(&self) -> bool {
        (self.head() % self.buffer_size()) == ((self.tail() + 1) % self.buffer_size())
    }

    /// Returns true if the FIFO is empty.
    pub fn is_empty(&self) -> bool {
        self.head() == self.tail()
    }

    /// Returns the current head index.
    fn head(&self) -> usize {
        unsafe { ptr::read_volatile(self.head.0.get()) as usize }
    }

    /// Returns the current tail index.
    fn tail(&self) -> usize {
        unsafe { ptr::read_volatile(self.tail.0.get()) as usize }
    }

    /// Sets the head index to the given value with memory ordering guarantees.
    fn set_head(&self, head: usize) {
        fence(Ordering::SeqCst);
        unsafe {
            ptr::write_volatile(self.head.0.get(), head as u32);
        }
        fence(Ordering::SeqCst);
    }

    /// Sets the tail index to the given value with memory ordering guarantees.
    fn set_tail(&self, tail: usize) {
        fence(Ordering::SeqCst);
        unsafe {
            ptr::write_volatile(self.tail.0.get(), tail as u32);
        }
        fence(Ordering::SeqCst);
    }

    /// Returns a non-null pointer to the buffer as a slice.
    fn buffer(&self) -> NonNull<[T]> {
        NonNull::slice_from_raw_parts(self.meta.0.buffer, self.buffer_size())
    }

    /// Returns the number of elements currently in the FIFO.
    fn num_elems(&self) -> usize {
        if self.head() > self.tail() {
            return self.head() - self.tail();
        } else {
            return self.capacity() + self.head() - self.tail();
        }
    }

    /// Returns the capacity of the FIFO.
    /// The capacity is the number of elements that can be stored in the FIFO.
    /// This is the size of the buffer minus one, as one slot is used to determine if the FIFO is full.
    pub fn capacity(&self) -> usize {
        self.buffer_size() - 1
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
    use std::thread;

    use super::CohortFifo;

    #[test]
    fn initializes_empty() {
        let spsc = CohortFifo::<[u8; 16]>::new(10).unwrap();
        assert!(spsc.is_empty());
    }

    #[test]
    fn test_filling_up_and_test_extra_push_and_test_emptying_and_test_extra_pop() {
        let spsc = CohortFifo::<[u8; 16]>::new(10).unwrap();

        for n in 0..10 {
            let val: [u8; 16] = [n; 16];
            spsc.push(&val);
        }

        spsc.print_queue();
        assert!(spsc.is_full());
        assert!(spsc.try_push(&[11; 16]).is_err());
        assert!(spsc.is_full());

        for n in 0..5 {
            let mut val = [0; 16];
            spsc.pop(&mut val);
            assert_eq!(val, [n; 16]);
        }

        for n in 0..5 {
            spsc.push(&mut [n; 16]);
        }

        for n in 5..10 {
            let mut val = [0; 16];
            spsc.pop(&mut val);
            assert!(val == [n; 16]);
        }

        for n in 0..5 {
            let mut val = [0; 16];
            spsc.pop(&mut val);
            assert!(val == [n; 16]);
        }
        assert!(spsc.is_empty());
        let mut val = [0; 16];
        assert!(spsc.try_pop(&mut val).is_err());
    }

    #[test]
    fn test_two_threads() {
        let spsc = CohortFifo::<[u8; 16]>::new(10).unwrap();

        thread::scope(|s| {
            const THROUGHPUT: u32 = 10_000_000;
            let handle = s.spawn(|| {
                for i in 0..THROUGHPUT {
                    spsc.push(&[(i % 64) as u8; 16]);
                }
            });

            for i in 0..THROUGHPUT {
                let mut elem = [0; 16];
                spsc.pop(&mut elem);
                assert_eq!(elem, [(i % 64) as u8; 16]);
            }
            assert!(spsc.is_empty());
        });
    }
}
