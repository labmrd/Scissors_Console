use crate::nidaqmx::task_handle::{RawTaskHandle, TaskHandle};

use std::collections::VecDeque;
use std::ptr;
use std::sync::{Arc, Weak as ArcWeak};

use futures::{Async, Poll, Stream};

use parking_lot::{Mutex, RwLock};

const SAMPLE_TIMEOUT_SECS: f64 = 1.0;
const BATCH_SIZE: usize = 10;
const SAMPLE_RATE: f64 = 100.0; // todo remove the need for this constant
const NUM_CHANNELS: usize = 2;
const VOLTAGE_SPAN: f64 = 10.0;

type RawScanData = [f64; NUM_CHANNELS];

struct BatchedScan {
	data: [RawScanData; BATCH_SIZE],
	timestamp: u64,
}

#[derive(Debug)]
pub struct ScanData {
	pub data: [f64; NUM_CHANNELS],
	pub timestamp: u64,
}

impl ScanData {
	fn new(data: [f64; NUM_CHANNELS], timestamp: u64) -> Self {
		ScanData { data, timestamp }
	}
}

struct AsyncAiChanInternal {
	buf: Mutex<VecDeque<BatchedScan>>,
	runtime_handle: RwLock<Option<futures::task::Task>>,
}

impl AsyncAiChanInternal {
	pub fn new() -> Self {
		AsyncAiChanInternal {
			buf: Mutex::new(VecDeque::new()),
			runtime_handle: RwLock::new(None),
		}
	}

	pub fn runtime_initialized(&self) -> bool {
		self.runtime_handle.read().is_some()
	}

	pub fn notify_data_ready(&self) -> Result<(), ()> {
		self.runtime_handle
			.try_read()
			.ok_or(())?
			.as_ref()
			.ok_or(())?
			.notify();

		Ok(())
	}

	pub fn initialize_runtime(&self, runtime: futures::task::Task) {
		let mut runtime_handle = self.runtime_handle.write();
		*runtime_handle = Some(runtime);
	}
}

pub struct AiChannel {
	task_handle: TaskHandle,
}

impl AiChannel {
	pub fn new(sample_rate: f64, clk_src: &str) -> Self {
		let task_handle = TaskHandle::new();

		let mut ai_channel = AiChannel { task_handle };

		ai_channel
			.task_handle
			.create_ai_volt_chan("Dev1/ai0:1", VOLTAGE_SPAN);
		ai_channel
			.task_handle
			.configure_sample_clock(clk_src, sample_rate, sample_rate as u64);

		ai_channel
	}

	pub fn make_async(self) -> AsyncAiChannel {
		let async_ai_chan_inner = Arc::new(AsyncAiChanInternal::new());

		let mut async_ai_chan = AsyncAiChannel {
			ai_chan: self,
			inner: async_ai_chan_inner,
			buf: VecDeque::new(),
		};

		let inner_weak_ptr = Arc::downgrade(&async_ai_chan.inner);

		unsafe {
			async_ai_chan.ai_chan.task_handle.register_read_callback(
				BATCH_SIZE as u32,
				async_read_callback_impl,
				inner_weak_ptr,
			);

			// We dont care about the done callback
			async_ai_chan
				.ai_chan
				.task_handle
				.register_done_callback(|_| (), ());
		}

		async_ai_chan.ai_chan.task_handle.launch();

		async_ai_chan
	}
}

pub struct AsyncAiChannel {
	ai_chan: AiChannel,
	inner: Arc<AsyncAiChanInternal>,
	buf: VecDeque<ScanData>,
}

impl Stream for AsyncAiChannel {
	type Item = ScanData;
	type Error = ();

	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
		loop {
			if !self.buf.is_empty() {
				return Ok(Async::Ready(self.buf.pop_front()));
			}

			let inner_buf = &mut *self.inner.buf.lock();

			if inner_buf.is_empty() {
				break;
			}

			for batch in inner_buf.drain(..) {
				const TO_NANOSEC: u64 = 1e9 as u64;
				let tstamp = batch.timestamp;

				let data = batch.data[..]
					.iter()
					.rev()
					.enumerate()
					.map(|(idx, sample)| {
						let ts_diff = idx as u64 * TO_NANOSEC / SAMPLE_RATE as u64;
						let actual_tstamp = tstamp - ts_diff;
						ScanData::new(*sample, actual_tstamp)
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

unsafe fn read_analog_f64(
	task_handle: &mut RawTaskHandle,
	buf: &mut BatchedScan,
	n_samps: u32,
) -> Result<(), i32> {
	let mut samps_read = 0i32;
	let samps_read_ptr = &mut samps_read as *mut _;

	buf.timestamp = time::precise_time_ns();

	let buf_len = buf.data.len() * buf.data[0].len();
	let buf_ptr = buf.data.as_mut_ptr() as *mut _;

	let err_code = nidaqmx_sys::DAQmxReadAnalogF64(
		task_handle.get().as_ptr(),
		n_samps as i32,
		SAMPLE_TIMEOUT_SECS,
		nidaqmx_sys::DAQmx_Val_GroupByScanNumber,
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
	inner_weak_ptr: &mut ArcWeak<AsyncAiChanInternal>,
	task_handle: &mut RawTaskHandle,
	n_samps: u32,
) -> Result<(), ()> {
	// If we can't upgrade, the task is complete
	let async_ai_inner = inner_weak_ptr.upgrade().ok_or(())?;

	let deque = &mut *async_ai_inner.buf.lock();

	deque.push_back(unsafe { std::mem::uninitialized() });

	let buf = deque.back_mut().unwrap();

	let read_result = unsafe { read_analog_f64(task_handle, buf, n_samps) };

	// Shortcircuit if we got an error and pop off the uninitalized data
	read_result.map_err(|err_code| {
		task_handle.chk_err_code(err_code);
		deque.pop_back();
	})?;

	// Try to notify the scheduler that we got data
	let _ = async_ai_inner.notify_data_ready();

	Ok(())
}
