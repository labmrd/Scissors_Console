use crate::nidaqmx::co_channel::*;
use crate::nidaqmx::task_handle::{RawTaskHandle, TaskHandle};
use crate::nidaqmx::{counter_generate_chan_desc, EMPTY_CSTRING};

use std::collections::VecDeque;
use std::ffi::CString;
use std::ptr;
use std::sync::{Arc, Weak as ArcWeak};

use futures::{Async, Poll, Stream};

use parking_lot::Mutex;

const CLK_SRC_OUTPUT_PFI_ID: u8 = 13;
const CLK_SRC_COUNTER_ID: u8 = 1;
const ENCODER_COUNTER_ID: u8 = 0;

const SAMPLE_TIMEOUT_SECS: f64 = 1.0;
const BATCH_SIZE: usize = 1000;
const DUTY_CYCLE: f64 = 0.5;

type RawScanData = [i32; BATCH_SIZE];

struct BatchedScan {
	data: RawScanData,
	timestamp: u64,
}

pub struct EncoderReading {
	pub timestamp: u64,
	pub pos: i32,
}

pub struct CiEncoderChannel {
	task_handle: TaskHandle,
	_co_channel: CoFreqChannel,
	sample_rate: f64,
}

impl CiEncoderChannel {
	pub fn new(sample_rate: f64) -> Self {
		let task_handle = TaskHandle::new();
		let _co_channel = CoFreqChannel::new(CLK_SRC_COUNTER_ID, sample_rate, DUTY_CYCLE);

		let mut ci_encoder_channel = CiEncoderChannel {
			task_handle,
			_co_channel,
			sample_rate,
		};

		ci_encoder_channel.setup();

		ci_encoder_channel
	}

	pub fn make_async(self) -> AsyncCiEncoderChannel {
		let async_internal = AsyncEncoderChanInternal {
			buf: Mutex::new(VecDeque::new()),
			runtime_handle: Mutex::new(None),
		};

		let mut async_encoder = AsyncCiEncoderChannel {
			encoder_chan: self,
			inner: Arc::new(async_internal),
			buf: VecDeque::new(),
		};

		let inner_weak_ptr = Arc::downgrade(&async_encoder.inner);

		unsafe {
			async_encoder
				.encoder_chan
				.task_handle
				.register_read_callback(
					BATCH_SIZE as u32,
					async_read_callback_impl,
					inner_weak_ptr,
				);

			// Don't care about the done callback
			async_encoder
				.encoder_chan
				.task_handle
				.register_done_callback(|_| (), ());
		}

		async_encoder.encoder_chan.task_handle.launch();

		async_encoder
	}

	fn setup(&mut self) {
		self.create_channel(ENCODER_COUNTER_ID);

		let clk_src = generate_clock_src_desc(CLK_SRC_OUTPUT_PFI_ID);
		self.task_handle.configure_sample_clock(
			&clk_src,
			self.sample_rate,
			self.sample_rate as u64,
		);
	}

	fn create_channel(&mut self, id: u8) {
		const USE_ENCODER_IDX: bool = true;
		const IDX_PULSE_POSITION: f64 = 0.0;
		const PULSE_PER_REV: u32 = 500;
		const INITIAL_POSITION: f64 = 0.0;

		let name_of_channel = EMPTY_CSTRING;
		let chan_desc = CString::new(counter_generate_chan_desc(id)).unwrap();

		let err_code = unsafe {
			nidaqmx_sys::DAQmxCreateCIAngEncoderChan(
				self.task_handle.get(),
				chan_desc.as_ptr(),
				name_of_channel,
				nidaqmx_sys::DAQmx_Val_X4 as i32,
				USE_ENCODER_IDX as u32,
				IDX_PULSE_POSITION,
				nidaqmx_sys::DAQmx_Val_ALowBLow as i32,
				nidaqmx_sys::DAQmx_Val_Ticks as i32,
				PULSE_PER_REV,
				INITIAL_POSITION,
				ptr::null_mut(),
			)
		};

		self.task_handle.chk_err_code(err_code);
	}
}

struct AsyncEncoderChanInternal {
	buf: Mutex<VecDeque<BatchedScan>>,
	runtime_handle: Mutex<Option<futures::task::Task>>,
}

impl AsyncEncoderChanInternal {
	pub fn runtime_initialized(&self) -> bool {
		self.runtime_handle.lock().is_some()
	}

	pub fn initialize_runtime(&self, runtime: futures::task::Task) {
		let mut runtime_handle = self.runtime_handle.lock();
		*runtime_handle = Some(runtime);
	}

	pub fn notify_data_ready(&self) -> Result<(), ()> {
		self.runtime_handle
			.try_lock() // try to acquire mutex
			.ok_or(())?
			.as_ref()
			.ok_or(())? // try to acquire handle
			.notify();
		Ok(())
	}
}

pub struct AsyncCiEncoderChannel {
	encoder_chan: CiEncoderChannel,
	inner: Arc<AsyncEncoderChanInternal>,
	buf: VecDeque<EncoderReading>,
}

impl Stream for AsyncCiEncoderChannel {
	type Item = EncoderReading;
	type Error = ();

	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {

		loop {
			if !self.buf.is_empty() {
				return Ok(Async::Ready(self.buf.pop_front()));
			}

			let mut inner_buf = match self.inner.buf.try_lock() {
				Some(inner) => inner,
				None => {
					futures::task::current().notify();
					return Ok(Async::NotReady);
				}
			};

			if inner_buf.is_empty() {
				break;
			}

			for batch in inner_buf.drain(..) {
				const TO_NANOSEC: u64 = 1e9 as u64;
				let tstamp = batch.timestamp;
				let sample_rate = self.encoder_chan.sample_rate as u64;

				let data = batch.data[..]
					.iter()
					.rev()
					.enumerate()
					.map(|(idx, sample)| {
						let ts_diff = idx as u64 * TO_NANOSEC / sample_rate;
						let actual_tstamp = tstamp - ts_diff;
						EncoderReading { pos: *sample, timestamp: actual_tstamp }
					})
					.rev();

				self.buf.extend(data);
			}

		}

		if !self.inner.runtime_initialized() {
			self.inner.initialize_runtime(futures::task::current());
		}

		Ok(Async::NotReady)
	}
}

fn generate_clock_src_desc(pfi_id: u8) -> String {
	let desc = format!("/Dev1/PFI{}", pfi_id);
	desc
}

unsafe fn read_digital_u32(
	task_handle: &mut RawTaskHandle,
	buf: &mut BatchedScan,
	n_samps: u32,
) -> Result<(), i32> {
	let mut samps_read = 0i32;
	let samps_read_ptr = &mut samps_read as *mut _;

	buf.timestamp = time::precise_time_ns();

	let buf_len = buf.data.len();
	let buf_ptr = buf.data.as_mut_ptr() as *mut u32; // pretend the i32 is a u32

	let err_code = nidaqmx_sys::DAQmxReadCounterU32(
		task_handle.get().as_ptr(),
		n_samps as i32,
		SAMPLE_TIMEOUT_SECS,
		buf_ptr,
		buf_len as u32,
		samps_read_ptr,
		ptr::null_mut(),
	);

	match err_code {
		0 if samps_read == BATCH_SIZE as i32 => Ok(()),
		_ => Err(err_code),
	}
}

fn async_read_callback_impl(
	inner_weak_ptr: &mut ArcWeak<AsyncEncoderChanInternal>,
	task_handle: &mut RawTaskHandle,
	n_samps: u32,
) -> Result<(), ()> {
	// If we can't upgrade, the task is complete
	let async_ai_inner = inner_weak_ptr.upgrade().ok_or(())?;

	let deque = &mut *async_ai_inner.buf.lock();

	deque.push_back(unsafe { std::mem::uninitialized() });

	let buf = deque.back_mut().unwrap();

	let read_result = unsafe { read_digital_u32(task_handle, buf, n_samps) };

	// Shortcircuit if we got an error and pop off the uninitalized data
	read_result.map_err(|err_code| {
		task_handle.chk_err_code(err_code);
		deque.pop_back();
	})?;

	// Try to notify the scheduler that we got data
	let _ = async_ai_inner.notify_data_ready();

	Ok(())
}
