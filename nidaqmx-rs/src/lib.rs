mod ai_channel;
mod task_handle;
mod co_channel;
mod callback_utils;
mod ci_encoder_channel;

const EMPTY_CSTRING: *const i8 = b"\0".as_ptr() as *const i8;
const DAQ_CALLBACK_FREQ: usize = 100; // hz
const SAMPLE_TIMEOUT_SECS: f64 = 1.0;
const SCAN_WARNING: i32 = i32::max_value();

#[allow(dead_code)]
const CALLBACK_PERIOD: u64 =		// DAQ Callback period [ns]
			1e9 as u64 /
			DAQ_CALLBACK_FREQ as u64;

// Time to subtract from timestamps; this is to prevent super massive numbers from being written
// to the data files. It avoids number precision issues, and also reduces file size.
// The time corresponds to July 11, 2019 at 1:45:00 PM (CST)
// in the 18-digit LDAP/FILETIME timestamp format.
const TIMEPOINT: u64 = 132080302920000000;

fn counter_generate_chan_desc(counter_id: u8) -> String {
	let desc = format!("Dev1/ctr{}", counter_id);
	desc
}

// Current time minus the TIMEPOINT in nanoseconds
pub fn get_steady_time_nanoseconds() -> u64 {
	get_steady_time_pretty_please_windows_i_swear_to_god()
}

use winapi::um::sysinfoapi::GetSystemTimePreciseAsFileTime;
use winapi::shared::minwindef::FILETIME;

// Current time minus the TIMEPOINT in nanoseconds
fn get_steady_time_pretty_please_windows_i_swear_to_god() -> u64 {
    let mut file_t = FILETIME::default();
    unsafe { GetSystemTimePreciseAsFileTime(&mut file_t) };
    
    let t = ((file_t.dwHighDateTime as u64) << 32) | file_t.dwLowDateTime as u64;

	let t = t - TIMEPOINT;

    t * 100 // windows returns time as 100s of ns, convert to ns
}

pub use ai_channel::*;
pub use ci_encoder_channel::*;
