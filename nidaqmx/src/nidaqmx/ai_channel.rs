use crate::nidaqmx::get_time_steady_nanoseconds;
use crate::nidaqmx::task_handle::{RawTaskHandle, TaskHandle};

use std::ptr;

use futures::{
	stream::Stream,
	sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
};

use generic_array::{ArrayLength, GenericArray};
use typenum::{Unsigned, U100};

const SAMPLE_TIMEOUT_SECS: f64 = 1.0;
const NUM_CHANNELS: usize = 2;
const VOLTAGE_SPAN: f64 = 10.0;

type DaqCallbackFreq = U100;
const DAQ_CALLBACK_FREQ: usize = DaqCallbackFreq::USIZE; // hz

type RawScanData = [f64; NUM_CHANNELS];

struct BatchedScan<N: ArrayLength<RawScanData>> {
	data: GenericArray<RawScanData, N>,
	timestamp: u64,
}

impl<N: ArrayLength<RawScanData>> BatchedScan<N> {
	pub unsafe fn new_uninit() -> Self {
		std::mem::uninitialized()
	}
}

impl<N: ArrayLength<RawScanData> + 'static> BatchedScan<N> {

	fn into_scan(self, sample_rate: u64) -> impl Iterator<Item = ScanData> {

		const TO_NANOSEC: u64 = 1e9 as u64;

		let base_ts = self.timestamp;

		let tstamp = (1..).map(move |ind| ind * TO_NANOSEC / sample_rate + base_ts);

		let scan_data = self.data.into_iter().rev().zip(tstamp).map(|(data, ts)| ScanData::new(data, ts));

		scan_data
	}

// 			for batch in inner_buf.drain(..) {
// 				const TO_NANOSEC: u64 = 1e9 as u64;
// 				let tstamp = batch.timestamp;

// 				let data = batch.data[..]
// 					.iter()
// 					.rev()
// 					.enumerate()
// 					.map(|(idx, sample)| {
// 						let ts_diff = idx as u64 * TO_NANOSEC / self.ai_chan.sample_rate as u64;
// 						let actual_tstamp = tstamp - ts_diff;
// 						ScanData::new(*sample, actual_tstamp)
// 					})
// 					.rev();

// 				self.buf.extend(data);
// 			}

}

#[derive(Debug)]
pub struct ScanData {
	pub data: RawScanData,
	pub timestamp: u64,
}

impl ScanData {
	fn new(data: [f64; NUM_CHANNELS], timestamp: u64) -> Self {
		ScanData { data, timestamp }
	}
}

pub struct AiChannel<SampleRate: Unsigned> {
	task_handle: TaskHandle,
	_phantom_data: std::marker::PhantomData<SampleRate>,
}

trait AsyncChannel<SampleRate>
where
	Self: Sized,
	Self::BatchSize: ArrayLength<RawScanData>,
	SampleRate: Unsigned,
{
	type BatchSize;
	fn new(self) -> UnboundedReceiver<BatchedScan<Self::BatchSize>>;
}

impl<SampleRate: Unsigned> AiChannel<SampleRate> {
	const SAMPLE_RATE: usize = SampleRate::USIZE;
	const NIDAQ_INTERNAL_SAMPLE_BUFFER_SIZE: u64 = 10 * Self::SAMPLE_RATE as u64;

	const BATCH_SIZE: usize = Self::SAMPLE_RATE / DAQ_CALLBACK_FREQ;

	pub fn new<S: AsRef<str>>(clk_src: S) -> Self {
		let task_handle = TaskHandle::new();

		let mut ai_channel = AiChannel {
			task_handle,
			_phantom_data: std::marker::PhantomData,
		};

		ai_channel
			.task_handle
			.create_ai_volt_chan("Dev1/ai0:1", VOLTAGE_SPAN);

		ai_channel.task_handle.configure_sample_clock(
			clk_src.as_ref(),
			Self::SAMPLE_RATE as f64,
			Self::NIDAQ_INTERNAL_SAMPLE_BUFFER_SIZE,
		);

		ai_channel
	}

	// pub fn make_async(self) {
	// 	// let async_ai_chan_inner = Arc::new(AsyncAiChanInternal::new());

	// 	// let mut async_ai_chan = AsyncAiChannel {
	// 	// 	ai_chan: self,
	// 	// 	inner: async_ai_chan_inner,
	// 	// 	buf: VecDeque::new(),
	// 	// };

	// 	// let inner_weak_ptr = Arc::downgrade(&async_ai_chan.inner);

	// 	// unsafe {
	// 	// 	async_ai_chan.ai_chan.task_handle.register_read_callback(
	// 	// 		BATCH_SIZE as u32,
	// 	// 		async_read_callback_impl,
	// 	// 		inner_weak_ptr,
	// 	// 	);

	// 	// 	// We dont care about the done callback
	// 	// 	async_ai_chan
	// 	// 		.ai_chan
	// 	// 		.task_handle
	// 	// 		.register_done_callback(|_| (), ());
	// 	// }

	// 	// async_ai_chan.ai_chan.task_handle.launch();

	// 	// async_ai_chan

	// 	let (snd, recv) = mpsc::unbounded();

	// 	type BatchSize = typenum::Quot<SampleRate, DaqCallbackFreq>;

	// 	unsafe {
	// 		self.task_handle.register_read_callback(
	// 			Self::BATCH_SIZE as u32,
	// 			async_read_callback_impl,
	// 			snd,
	// 		);
	// 		// We dont care about the done callback
	// 		self.task_handle.register_done_callback(|_| (), ());
	// 	}

	// 	self.task_handle.launch();
	// }
}

// impl Stream for AsyncAiChannel {
// 	type Item = ScanData;
// 	type Error = ();

// 	fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
// 		loop {
// 			if !self.buf.is_empty() {
// 				return Ok(Async::Ready(self.buf.pop_front()));
// 			}

// 			let inner_buf = &mut *self.inner.buf.lock();

// 			if inner_buf.is_empty() {
// 				break;
// 			}

// 			for batch in inner_buf.drain(..) {
// 				const TO_NANOSEC: u64 = 1e9 as u64;
// 				let tstamp = batch.timestamp;

// 				let data = batch.data[..]
// 					.iter()
// 					.rev()
// 					.enumerate()
// 					.map(|(idx, sample)| {
// 						let ts_diff = idx as u64 * TO_NANOSEC / self.ai_chan.sample_rate as u64;
// 						let actual_tstamp = tstamp - ts_diff;
// 						ScanData::new(*sample, actual_tstamp)
// 					})
// 					.rev();

// 				self.buf.extend(data);
// 			}
// 		}

// 		if !self.inner.runtime_initialized() {
// 			self.inner.initialize_runtime(futures::task::current());
// 		}

// 		Ok(Async::NotReady)
// 	}
// }

use std::ops::Div;
use typenum::Quot;

impl<SampleRate> AsyncChannel<SampleRate> for AiChannel<SampleRate>
where
	SampleRate: Unsigned + Div<DaqCallbackFreq>,
	Quot<SampleRate, DaqCallbackFreq>: ArrayLength<RawScanData>,
{
	type BatchSize = typenum::Quot<SampleRate, DaqCallbackFreq>;

	fn new(mut self) -> UnboundedReceiver<BatchedScan<Self::BatchSize>> {
		let (snd, recv) = mpsc::unbounded();

		unsafe {
			self.task_handle.register_read_callback(
				Self::BATCH_SIZE as u32,
				async_read_callback_impl::<Self::BatchSize>,
				snd,
			);
			// We dont care about the done callback
			self.task_handle.register_done_callback(|_| (), ());
		}

		self.task_handle.launch();

		recv
	}
}

unsafe fn read_analog_f64<N: ArrayLength<RawScanData>>(
	task_handle: &mut RawTaskHandle,
	n_samps: u32,
) -> Result<BatchedScan<N>, i32> {
	const SCAN_WARNING: i32 = 1;

	let mut samps_read = 0i32;
	let samps_read_ptr = &mut samps_read as *mut _;

	let mut scan = BatchedScan::<N>::new_uninit();
	scan.timestamp = get_time_steady_nanoseconds();

	let buf_len = (N::USIZE * NUM_CHANNELS) as u32;
	let buf_ptr = scan.data.as_mut_slice() as *mut _ as *mut f64;

	let err_code = nidaqmx_sys::DAQmxReadAnalogF64(
		task_handle.get().as_ptr(),
		n_samps as i32,
		SAMPLE_TIMEOUT_SECS,
		nidaqmx_sys::DAQmx_Val_GroupByScanNumber,
		buf_ptr,
		buf_len,
		samps_read_ptr,
		ptr::null_mut(),
	);

	match err_code {
		0 if samps_read == N::I32 => Ok(scan),
		0 => Err(SCAN_WARNING),
		_ => Err(err_code),
	}
}

fn async_read_callback_impl<N>(
	send_channel: &mut UnboundedSender<BatchedScan<N>>,
	task_handle: &mut RawTaskHandle,
	n_samps: u32,
) -> Result<(), ()>
where
	N: ArrayLength<RawScanData>,
{
	let result = unsafe { read_analog_f64(task_handle, n_samps) };

	result
		.map(|data| send_channel.unbounded_send(data).map_err(|_| ()))
		.map_err(|err_code| task_handle.chk_err_code(err_code))?
}
