use crate::util::Aligned;
use core::ptr::NonNull;
use std::{
    alloc::{alloc_zeroed, dealloc, Layout},
    cell::UnsafeCell,
    mem, ptr,
};
use std::sync::atomic::{fence, Ordering};


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

impl<T: Copy + std::fmt::Debug> CohortFifo<T> {
    // Creates new fifo.
    pub fn new(capacity: usize, batch_size: usize) -> Result<Self, &'static str> {
        if (batch_size < 2){
            return Err("Arg `batch_size` cannot be less than 2")
        }

        if (batch_size % 2 != 0){
            return Err("Arg `batch_size` must be even")
        }

        if(capacity < batch_size) {
            return Err("Arg `capacity` cannot be less than `batch_size`")
        }
        // Capacity must 
        if(capacity %2 != 0){
            return Err("Arg `capacity` must be divisible by 2.");
        }
        let buffer = unsafe {
            let buffer_size = capacity + 1;
            let layout = Layout::array::<T>(buffer_size).unwrap();
            let aligned = layout.align_to(128).unwrap();
            NonNull::new(alloc_zeroed(aligned)).unwrap()
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

    pub fn try_push(&self, elem1: &T, elem2: &T) -> Result<(), ()> {
        if self.is_full() {
            return Err(());
        }
        // println!("-----SENDER QUEUE------");
        // self.print_queue();
        let sw_tail = self.sw_tail();
        unsafe {
            (*self.buffer().as_ptr())[sw_tail] = *elem1;
            (*self.buffer().as_ptr())[(sw_tail+1) %self.buffer_size()] = *elem2;
        }

        self.set_sw_tail((sw_tail + 2) % self.buffer_size());

        // Make sure the hw_tail keeps up when we go over the batch
        // size, this optimizes the accelerator by allowing it 
        // to process large batches at a time.
        if self.num_elems() >= self.batch_size {
            self.set_hw_tail(self.sw_tail());
        }

        Ok(())
    }

    /// Pushes an element to the fifo.
    pub fn push(&self, elem1: &T, elem2: &T) {
        while self.try_push(elem1, elem2).is_err() {}
    }

    pub fn try_pop(&self, elem1: &mut T, elem2: &mut T) -> Result<(), ()> {
        // If we're popping that means we're a receiver queue
        // And we don't need to worry about batch sizes so just automatically
        // update the sw_tail to the hw_tail before doing anything
        self.set_sw_tail(self.hw_tail());

        // Ensure that the accelerator has pushed at least two elements onto the queue
        if self.is_empty() || self.num_elems() == 1 {
            // println!("NUMBER OF ELEMS: {}", self.num_elems());
            return Err(());
        }
        // println!("---------RECEIVER QUEUE--------");
        // self.print_queue();
        let head = self.head();
        *elem1 = unsafe { (*self.buffer().as_ptr())[head]};
        *elem2 = unsafe {(*self.buffer().as_ptr())[(head+1) %self.buffer_size()]};

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

    pub fn print_queue(&self){
       unsafe{ println!("{:?}", self.buffer().as_ref())};
    }
    

    /// True size of the underlying buffer.
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

    fn sw_tail(&self) -> usize {
        unsafe { ptr::read_volatile(self.sw_tail.0.get()) as usize }
    }

    fn hw_tail(&self) -> usize {
        unsafe { ptr::read_volatile(self.hw_tail.0.get()) as usize }
    }

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
            ptr::write_volatile(self.sw_tail.0.get(), tail as u32);
        }
        fence(Ordering::SeqCst);

    }

    fn buffer(&self) -> NonNull<[T]> {
        NonNull::slice_from_raw_parts(self.meta.0.buffer, self.buffer_size())
    }


    fn capacity(&self) -> usize {
        self.buffer_size()-1
    }
}

unsafe impl<T: Copy + std::fmt::Debug> Send for CohortFifo<T> {}
unsafe impl<T: Copy + std::fmt::Debug> Sync for CohortFifo<T>{}

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
    fn test_filling_up_and_test_extra_push_and_test_emptying_and_test_extra_pop(){
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
            let mut val = [0;16];
            spsc.pop(&mut val);
            assert_eq!(val, [n;16]);
        }

        for n in 0..5 {
            spsc.push(&mut [n;16]);
        }

        for n in 5..10 {
            let mut val = [0;16];
            spsc.pop(&mut val);
            assert!(val == [n;16]);
        }

        for n in 0..5 {
            let mut val = [0;16];
            spsc.pop(&mut val);
            assert!(val == [n;16]);
        }
        assert!(spsc.is_empty());
        let mut val = [0;16];
        assert!(spsc.try_pop(&mut val).is_err());
    }

    #[test]
    fn test_two_threads(){
        let spsc = CohortFifo::<[u8;16]>::new(10).unwrap();

        thread::scope( |s| {
            const THROUGHPUT: u32 = 10_000_000;
            let handle = s.spawn(|| {
            for i in 0..THROUGHPUT {
                spsc.push(&[(i%64) as u8;16]);
            }
        });

        for i in 0..THROUGHPUT {
            let mut elem =[0;16];
            spsc.pop(&mut elem);
            assert_eq!(elem, [(i%64) as u8;16]);
        }
        assert!(spsc.is_empty());
       
    });

    }
}