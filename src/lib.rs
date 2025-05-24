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
// // #![feature(atomic_from_mut)]
#![warn(missing_docs)]
// // #![feature(ptr_as_uninit)]

/// Error types used by the Cohort crate.
pub mod error;
mod fifo;
pub(crate) mod util;

use core::marker::PhantomPinned;
use core::pin::Pin;
use core::sync::atomic::AtomicU64;

use fifo::CohortFifo;

use crate::util::Aligned;

const COHORT_REGISTER_SYSCALL: libc::c_int = 258;
const COHORT_UNREGISTER_SYSCALL: libc::c_int = 257;
pub const BACKOFF_COUNTER_VAL: u64 = 240;

/// A specialized `Result` type for FIFO operations, using [`Error`] as the error type.
pub type Result<T> = std::result::Result<T, crate::error::Error>;

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
    custom_data: Aligned<AtomicU64>, // TODO: Determine type
    // Prevents compiler from implementing unpin trait
    _pin: PhantomPinned,
}

impl<T: Copy + std::fmt::Debug> Cohort<T> {
    /// Creates a new cohort with the provided id and capacity.
    /// Will not register the cohort with the kernel.
    ///
    /// # Safety
    ///
    /// The cohort id must not currently be in use.
    pub unsafe fn new(id: u8, capacity: usize, batch_size: usize) -> Pin<Box<Self>> {
        let sender = CohortFifo::new(capacity, batch_size).unwrap();

        // Batch size doesn't matter for the receiver because we are not pushing data
        // onto the receiver queue
        let receiver = CohortFifo::new(capacity, batch_size).unwrap();
        let custom_data = Aligned(AtomicU64::new(0));

        Box::pin(Cohort {
            _id: id,
            sender,
            receiver,
            custom_data,
            _pin: PhantomPinned,
        })
    }

    /// Registers a cohort with the provided id with the given capacity.
    /// This wraps (new)[Self::new] and calls (cohort_mn_register)[Self::cohort_mn_register].
    /// It is recommended to use this function instead of (new)[Self::new] directly,
    /// as this function does not return the integer value of cohort_mn_register.
    ///
    /// # Safety
    ///
    /// The cohort id must not currently be in use.
    pub unsafe fn register(id: u8, capacity: usize, batch_size: usize) -> Pin<Box<Self>> {
        let cohort = Self::new(id, capacity, batch_size);
        cohort.cohort_mn_register();
        cohort
    }

    /// Calls the syscall to register the cohort with the kernel.
    pub fn cohort_mn_register(&self) -> libc::c_int {
        unsafe {
            libc::syscall(
                COHORT_REGISTER_SYSCALL,
                &self.sender,
                &self.receiver,
                &(self.custom_data.0),
                BACKOFF_COUNTER_VAL,
            )
        }
    }

    /// Calls the syscall to unregister the cohort with the kernel.
    /// This is called automatically when the cohort is dropped.
    /// It is recommended to call this function manually if the cohort is not dropped.
    pub fn cohort_mn_unregister(&self) -> libc::c_int {
        unsafe { libc::syscall(COHORT_UNREGISTER_SYSCALL) }
    }

    /// Sends an element to the accelerator.
    ///
    /// Spins if the sending end is full.
    pub fn push(&self, elem1: &T, elem2: &T) {
        self.sender.push(elem1, elem2);
    }

    /// Receives an element from the accelerator.
    ///
    /// Spins if the receiving end is full.
    pub fn pop(&self, elem1: &mut T, elem2: &mut T) {
        self.receiver.pop(elem1, elem2)
    }

    /// Sends an element to the accelerator.
    ///
    /// Will fail if the sending end is full.
    pub fn try_push(&self, elem1: &T, elem2: &T) -> Result<()> {
        self.sender.try_push(elem1, elem2)
    }

    /// Receives an element from the accelerator.
    ///
    /// Will fail if receiving end is full.
    pub fn try_pop(&self, elem1: &mut T, elem2: &mut T) -> Result<()> {
        self.receiver.try_pop(elem1, elem2)
    }

    /// Returns the receiver FIFO associated with the cohort.
    pub fn receiver(&self) -> &CohortFifo<T> {
        &self.receiver
    }

    /// Returns the sender FIFO associated with the cohort.
    pub fn sender(&self) -> &CohortFifo<T> {
        &self.sender
    }

    /// Returns the custom data associated with the cohort.
    pub fn custom_data(&self) -> &Aligned<AtomicU64> {
        &self.custom_data
    }

    /// Returns a string representation of the receiver FIFO.
    pub fn receiver_to_string(&self) -> String {
        format!("{:?}", self.receiver)
    }

    /// Returns a string representation of the sender FIFO.
    pub fn sender_to_string(&self) -> String {
        format!("{:?}", self.sender)
    }
}

impl<T: Copy + std::fmt::Debug> Drop for Cohort<T> {
    fn drop(&mut self) {
        //TODO: check status from syscall
        self.cohort_mn_unregister();
    }
}
