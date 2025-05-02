//! The Cohort crate provides an high-level interface to Cohort.
//!
//! Cohort enables simple and efficent communication to various hardware
//! accelerators through a software-oriented acceleration (SOA) approach.
//!
//! For more information see: [Cohort: Software-Oriented Acceleration for Heterogeneous SoCs](https://dl.acm.org/doi/10.1145/3582016.3582059)
//!
//! # Examples
//!
//! ```no_run
//! // SAFETY: No other cohorts are associated with id 0.
//! let cohort = unsafe { Cohort::register(0, 32) };
//! // Send data to the accelerator.
//! cohort.push(10);
//! // Get data from the accelerator.
//! let data = cohort.pop();
//! ```
#![feature(atomic_from_mut)]
#![warn(missing_docs)]
#![feature(ptr_as_uninit)]

mod fifo;
pub(crate) mod util;

use core::marker::PhantomPinned;
use core::pin::Pin;
use core::sync::atomic::AtomicU64;

use fifo::CohortFifo;

use crate::util::Aligned;

const BACKOFF_COUNTER_VAL: u64 = 240;


/// a single-producer, single-consumer (SPSC) interface used to communciate with hardware accelerators.
///
/// ```no_run
/// // SAFETY: No other cohorts are associated with id 0.
/// let cohort = unsafe { Cohort::register(0, 32) };
/// // Send data to the accelerator.
/// cohort.push(10);
/// // Get data from the accelerator.
/// let data = cohort.pop();
/// ```
pub struct Cohort<T: Copy + std::fmt::Debug> {
    _id: u8,
    sender: CohortFifo<T>,
    receiver: CohortFifo<T>,
    custom_data: Aligned<AtomicU64>, //TODO: Determine type
    // Prevents compiler from implementing unpin trait
    _pin: PhantomPinned,
}

impl<T: Copy + std::fmt::Debug> Cohort<T> {
    /// Registers a cohort with the provided id with the given capacity.
    ///
    /// # Safety
    ///
    /// The cohort id must not currently be in use.
    pub unsafe fn register(id: u8, capacity: usize) -> Pin<Box<Self>> {
        let sender = CohortFifo::new(capacity).unwrap();
        let receiver = CohortFifo::new(capacity).unwrap();
        let custom_data = Aligned(AtomicU64::new(0));

        let cohort = Box::pin(Cohort {
            _id: id,
            sender,
            receiver,
            custom_data,
            _pin: PhantomPinned,
        });

        unsafe {
            libc::syscall(
                258,
                &cohort.sender,
                &cohort.receiver,
                &(cohort.custom_data.0),
                BACKOFF_COUNTER_VAL,
            );
        }

        cohort
    }

    /// Sends an element to the accelerator.
    ///
    /// May block if the sending end is full.
    pub fn push(&self, elem1: &T, elem2: &T) {
        self.sender.push(elem1, elem2);
    }

    /// Receives an element from the accelerator.
    ///
    /// May block if the receiving end is full.
    pub fn pop(&self, elem1: &mut T, elem2: &mut T) {
        self.receiver.pop(elem1, elem2)
    }

    /// Sends an element to the accelerator.
    ///
    /// Will fail if the sending end is full.
    pub fn try_push(&self, elem1: &T, elem2: &T) -> Result<(), ()> {
        self.sender.try_push(elem1, elem2)
    }


    /// Receives an element from the accelerator.
    ///
    /// Will fail if receiving end is full.
    pub fn try_pop(&self, elem1: &mut T, elem2: &mut T) -> Result<(), ()> {
        self.receiver.try_pop(elem1, elem2)
    }

    pub fn print_receiver(&self){
        self.receiver.print_queue();
    }

    pub fn print_sender(&self){
        self.sender.print_queue();
    }

}

impl<T: Copy + std::fmt::Debug> Drop for Cohort<T> {
    fn drop(&mut self) {
        unsafe {
            //TODO: check status from syscall
            libc::syscall(257);
        }
    }
}
