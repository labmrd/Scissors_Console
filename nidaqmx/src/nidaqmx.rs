mod ai_channel;
mod task_handle;
mod co_channel;
mod callback_utils;
mod ci_encoder_channel;

const EMPTY_CSTRING: *const i8 = b"\0".as_ptr() as *const i8;

pub fn counter_generate_chan_desc(counter_id: u8) -> String {
	let desc = format!("Dev1/ctr{}", counter_id);
	desc
}

pub use crate::nidaqmx::ai_channel::*;
pub use crate::nidaqmx::ci_encoder_channel::*;
