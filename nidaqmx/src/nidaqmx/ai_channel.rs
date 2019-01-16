use crate::nidaqmx::get_time_steady_nanoseconds;
use crate::nidaqmx::task_handle::{RawTaskHandle, TaskHandle};

use std::ptr;

use futures::{
	stream::Stream,
	sync::mpsc::{self, UnboundedReceiver, UnboundedSender},
};

use generic_array::{ArrayLength, GenericArray};
use typenum::Unsigned;

const SAMPLE_TIMEOUT_SECS: f64 = 1.0;
const NUM_CHANNELS: usize = 2;
const VOLTAGE_SPAN: f64 = 10.0;

type DaqCallbackFreq = typenum::U100;
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

impl<N: ArrayLength<RawScanData>> BatchedScan<N> {
	fn into_scan(self, sample_rate: u64) -> impl Iterator<Item = ScanData> {
		const TO_NANOSEC: u64 = 1e9 as u64;

		let base_ts = self.timestamp;

		let tstamp = (1..).map(move |ind| ind * TO_NANOSEC / sample_rate + base_ts);

		let scan_data = self
			.data
			.into_iter()
			.rev()
			.zip(tstamp)
			.map(|(data, ts)| ScanData::new(data, ts));

		scan_data
	}
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

pub struct AsyncAiStream<BatchSize: ArrayLength<RawScanData>> {
	inner: UnboundedReceiver<BatchedScan<BatchSize>>,
	sample_rate: u64,
}

impl<N: ArrayLength<RawScanData>> AsyncAiStream<N> {
	fn get(self) -> impl Stream<Item = ScanData, Error = ()> {
		let samp_rate = self.sample_rate;
		self.inner
			.map(move |batch| batch.into_scan(samp_rate))
			.map(|scan_iter| futures::stream::iter_ok(scan_iter))
			.flatten()
	}
}

trait AsyncChannel<SampleRate>
where
	Self: Sized,
	Self::BatchSize: ArrayLength<RawScanData>,
	SampleRate: Unsigned,
{
	type BatchSize;
	fn new(self) -> AsyncAiStream<Self::BatchSize>;
}

impl<SampleRate> AiChannel<SampleRate>
where
	SampleRate: Unsigned,
{
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

	pub fn make_async(self) -> impl Stream<Item = ScanData, Error = ()> {
		AsyncChannel::new(self).get()
	}
}

use std::ops::Div;
use typenum::Quot;

impl<SampleRate> AsyncChannel<SampleRate> for AiChannel<SampleRate>
where
	SampleRate: Unsigned + Div<DaqCallbackFreq>,
	Quot<SampleRate, DaqCallbackFreq>: ArrayLength<RawScanData>,
{
	type BatchSize = Quot<SampleRate, DaqCallbackFreq>;

	fn new(mut self) -> AsyncAiStream<Self::BatchSize> {
		let (snd, recv) = mpsc::unbounded();

		unsafe {
			self.task_handle.register_read_callback(
				Self::BatchSize::U32,
				async_read_callback_impl::<Self::BatchSize>,
				snd,
			);
			// We dont care about the done callback
			self.task_handle.register_done_callback(|_| (), ());
		}

		self.task_handle.launch();

		AsyncAiStream {
			inner: recv,
			sample_rate: SampleRate::U64,
		}
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
