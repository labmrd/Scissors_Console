#![feature(try_from)]

use tokio::codec::{FramedRead, LinesCodec};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

use std::convert::TryFrom;
use std::io::{self, Write};
use std::net::SocketAddr;
use std::sync::Arc;

use parking_lot::Mutex;

use log::{Level, Metadata, Record};

type WriteableSocket = tokio::io::WriteHalf<tokio::net::TcpStream>;

struct SocketHandle {
	inner: Arc<Mutex<WriteableSocket>>,
}

impl From<WriteableSocket> for SocketHandle {
	fn from(socket: WriteableSocket) -> SocketHandle {
		SocketHandle {
			inner: Arc::new(Mutex::new(socket)),
		}
	}
}

impl Clone for SocketHandle {
	fn clone(&self) -> Self {
		Self {
			inner: self.inner.clone(),
		}
	}
}

impl Write for SocketHandle {
	fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
		let mut socket_lock = self.inner.lock();
		let socket = &mut *socket_lock;

		socket.write(buf)
	}

	fn flush(&mut self) -> io::Result<()> {
		let mut socket_lock = self.inner.lock();
		let socket = &mut *socket_lock;

		socket.flush()
	}
}

impl AsyncWrite for SocketHandle {
	fn shutdown(&mut self) -> Result<Async<()>, tokio::io::Error> {
		let mut socket_lock = self.inner.lock();
		let socket = &mut *socket_lock;

		socket.shutdown()
	}

	fn poll_write(&mut self, buf: &[u8]) -> Result<Async<usize>, tokio::io::Error> {
		let mut socket_lock = self.inner.lock();
		let socket = &mut *socket_lock;

		socket.poll_write(buf)
	}

	fn poll_flush(&mut self) -> Result<Async<()>, tokio::io::Error> {
		let mut socket_lock = self.inner.lock();
		let socket = &mut *socket_lock;

		socket.poll_flush()
	}
}

struct GuiLogger {
	connection: Mutex<Option<SocketHandle>>,
}

impl GuiLogger {
	const fn new() -> Self {
		GuiLogger {
			connection: Mutex::new(None),
		}
	}

	fn register_socket(&self, socket: WriteableSocket) {
		*self.connection.lock() = Some(socket.into());
	}
}

impl log::Log for GuiLogger {
	fn enabled(&self, metadata: &Metadata) -> bool {
		metadata.level() <= Self::LOG_LEVEL
	}

	fn log(&self, record: &Record) {
		if !self.enabled(record.metadata()) {
			return;
		}

		let connection = self.connection.lock().as_ref().map(SocketHandle::clone);

		if let Some(socket) = connection {
			return;
			let msg = format!("{}\t{}", record.level(), record.args());

			tokio::spawn(
				tokio::io::write_all(socket, msg)
					.map(|_| ())
					.map_err(|_| ()),
			);
		} else {
			println!("Server stdout: {}\t{}", record.level(), record.args());
		}
	}

	fn flush(&self) {}
}

impl GuiLogger {
	#[cfg(debug_assertions)]
	const LOG_LEVEL: Level = Level::Trace;

	#[cfg(not(debug_assertions))]
	const LOG_LEVEL: Level = Level::Warn;
}

const LOCALHOST: [u8; 4] = [127, 0, 0, 1];
const PORT: u16 = 8080;

const ADDR: ([u8; 4], u16) = (LOCALHOST, PORT);

static LOGGER: GuiLogger = GuiLogger::new();

fn process_connection(tcp_stream: TcpStream) -> Result<(), ()> {
	let (reader, writer) = tcp_stream.split();

	LOGGER.register_socket(writer);

	let reader = FramedRead::new(reader, LinesCodec::new());

	let processor = reader
		.for_each(move |msg| {
			log::trace!("Got {}", &msg);

			let uie = match events::Client::try_from(msg.as_str()) {
				Ok(uie) => uie,
				Err(_) => return Ok(()),
			};

			log::trace!("{:#?}", uie);

			Ok(())
		})
		.and_then(|()| {
			log::trace!("Socket received FIN packet and closed connection");
			Ok(())
		})
		.or_else(|err| {
			log::trace!("Socket closed with error: {:?}", err);
			Err(err)
		})
		.then(|result| {
			log::trace!("Socket closed with result: {:?}", result);
			Ok(())
		});

	tokio::spawn(processor);
	Ok(())
}

fn main() -> Result<(), Box<std::error::Error>> {
	let _ = log::set_logger(&LOGGER).map(|_| log::set_max_level(log::LevelFilter::max()));

	let addr = SocketAddr::from(ADDR);
	let socket = TcpListener::bind(&addr)?;

	log::trace!("Listening on: {}", addr);
	
	let connection_daemon = socket
		.incoming()
		.map_err(|_| ())
		.for_each(move |socket| process_connection(socket));

	tokio::run(connection_daemon);

	Ok(())
}

// use std::io::Write;
// use futures::{stream, Stream};
// use futures::future::Future;
// use tokio::runtime::Runtime;

// use nidaqmx::{AiChannel, CiEncoderChannel, EncoderReading};

// // fn enc_stream(mut enc_chan: CiEncoderChannel) -> impl Stream<Item = EncoderReading, Error = ()> {

// // 	let iters = 0..;

// // 	stream::iter_ok(iters).map(move |_| {
// // 		enc_chan.acquire_sample()
// // 	})
// // }

// const SAMPLING_FREQ: f64 = 1e5;

// fn main() {

// 	let mut enc_file = std::io::BufWriter::with_capacity(1024 * 1024, std::fs::File::create("enc_data.csv").unwrap());
// 	let mut adc_file = std::io::BufWriter::with_capacity(1024 * 1024, std::fs::File::create("adc_data.csv").unwrap());

// 	let encoder_chan = CiEncoderChannel::new(SAMPLING_FREQ).make_async();
// 	let ai_chan = AiChannel::new(SAMPLING_FREQ, "/Dev1/PFI13").make_async();

// 	let encoder_stream = encoder_chan.for_each(move |val| {
// 		let _ = writeln!(&mut enc_file, "{},{}", val.timestamp, val.pos);
// 		Ok(())
// 	});

// 	let ai_stream = ai_chan.for_each(move |val| {
// 		let _ = writeln!(&mut adc_file, "{},{},{}", val.timestamp, val.data[0], val.data[1]);
// 		Ok(())
// 	});

// 	let mut runtime = Runtime::new().unwrap();

// 	runtime.spawn(ai_stream);
// 	runtime.spawn(encoder_stream);

// 	println!("Started data collection");

// 	runtime.shutdown_on_idle().wait().unwrap();

// 	println!("End of program.");
// }
