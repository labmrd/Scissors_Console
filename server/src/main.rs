#![feature(try_from)]

use tokio::codec::{FramedRead, LinesCodec};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

use std::convert::TryFrom;

use std::net::SocketAddr;

use parking_lot::Mutex;

use log::{Level, Metadata, Record};

use events::Event;

type SocketTxHandle = futures::sync::mpsc::UnboundedSender<events::Server>;

struct GuiLogger {
	connection: Mutex<Option<SocketTxHandle>>,
}

impl GuiLogger {
	const fn new() -> Self {
		GuiLogger {
			connection: Mutex::new(None),
		}
	}

	fn register_socket(&self, tx: SocketTxHandle) {
		*self.connection.lock() = Some(tx);
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

		let mut connection_lock = self.connection.lock();
		let connection = &*connection_lock;

		if let Some(tx) = connection {
			let msg = format!("{}\t{}", record.level(), record.args());
			let _ = tx.unbounded_send(events::Server::Msg(msg)).map_err(|_| *connection_lock = None);
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

	let (tx, rx) = futures::sync::mpsc::unbounded::<events::Server>();

	let log_tx = tx.clone();

	LOGGER.register_socket(log_tx);

	let reader = FramedRead::new(reader, LinesCodec::new());

	let channel_reader = rx
		.filter_map(|ev| ev.as_bytes())
		.fold(writer, |writer, event| {
			let amt_written = tokio::io::write_all(writer, event)
				.map(|(writer, _)| writer)
				.map_err(|_| ());
			amt_written
		}).map(|_| ());

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
		.and_then(|_| {
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
	tokio::spawn(channel_reader);

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
