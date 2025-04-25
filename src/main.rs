#![feature(slice_as_chunks)]

use cohort::Cohort;

const NUM_WORDS: usize = 32;
const FIFO_SIZE: usize = 64;
const BATCH_SIZE: usize = 8;
const OUT_BATCH_SIZE : usize = BATCH_SIZE;



//TODO: needs to be updated to use new function signatures defined in lib.rs,
// These write to the arguments when popping and read from a reference to the argument
// when pushing
fn main() {
//     const PLAIN: [u64; NUM_WORDS] =  [
//     0xFFFFFFFFFFFFFFFFu64,0x0000000033221100u64,
//     0xFFFFFFFFFFFFFFFFu64,0x0000000077665544u64,
//     0xFFFFFFFFFFFFFFFFu64,0x00000000BBAA9988u64,
//     0xFFFFFFFFFFFFFFFFu64,0x00000000FFEEDDCCu64,
//     0xFFFFFFFFFFFFFFFFu64,0x0000000011111111u64,
//     0xFFFFFFFFFFFFFFFFu64,0x0000000022222222u64,
//     0xFFFFFFFFFFFFFFFFu64,0x0000000033333333u64,
//     0xFFFFFFFFFFFFFFFFu64,0x0000000044444444u64,
//     0xFFFFFFFFFFFFFFFFu64,0x0000000055555555u64,
//     0xFFFFFFFFFFFFFFFFu64,0x0000000066666666u64,
//     0xFFFFFFFFFFFFFFFFu64,0x0000000077777777u64,
//     0xFFFFFFFFFFFFFFFFu64,0x0000000088888888u64,
//     0xFFFFFFFFFFFFFFFFu64,0x0000000099999999u64,
//     0xFFFFFFFFFFFFFFFFu64,0x00000000AAAAAAAAu64,
//     0xFFFFFFFFFFFFFFFFu64,0x00000000BBBBBBBBu64,
//     0xFFFFFFFFFFFFFFFFu64,0x00000000CCCCCCCCu64];

//     let mut accumulator : u64 = 0;


//     // SAFETY: No other cohorts are associated with id 0.
//     let  cohort: std::pin::Pin<Box<Cohort<[u8; 16]>>> = unsafe { Cohort::register(0, 64) };
//     let mut arr1: [u8; 16] = [1; 16];
//     let mut arr2: [u8; 8] = [1; 8];

//     let (chunks, remainder) = arr1.as_chunks_mut();

//     for chunk in chunks {
//         cohort.try_pop_write(chunk, &mut arr2);
//     }
//     cohort.try_pop_write(&mut arr1[0..8], &mut arr2[0..8]);
//     for k in 0..FIFO_SIZE/BATCH_SIZE{
//         for j in 0..BATCH_SIZE/2{
//             cohort.push(PLAIN[(k*BATCH_SIZE+j*2)%NUM_WORDS], PLAIN[(k*BATCH_SIZE+j*2+1)%NUM_WORDS]);
//         }
        
//         for j in 0..OUT_BATCH_SIZE/2{
//             let (elem1, elem2) = cohort.pop();

//             let mut idx = k*BATCH_SIZE+j*2;
//             println!("index:{idx} value:{:X}", elem1);
//             idx+=1;
//             println!("index:{idx} value:{:X}", elem2)
//         }
//     }
}
