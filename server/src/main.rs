#![feature(try_from)]

use tokio::codec::{FramedRead, LinesCodec};
use tokio::net::{TcpListener, TcpStream};
use tokio::prelude::*;

use std::convert::TryFrom;
use std::net::SocketAddr;

use events::UiEvent;

const LOCALHOST: [u8; 4] = [127, 0, 0, 1];
const PORT: u16 = 8080;

const ADDR: ([u8; 4], u16) = (LOCALHOST, PORT);

fn process_connections(tcp_stream: TcpStream) -> Result<(), ()> {
	// let framed = BytesCodec::new().framed(tcp_stream);

	let (reader, mut writer) = tcp_stream.split();

	let reader = FramedRead::new(reader, LinesCodec::new());

	let processor = reader
		.for_each(move |msg| {
			println!("Got {}", &msg);

			let echo = tokio::io::write_all(&mut writer, &msg).map(|_| ()).map_err(|_| ());
			let _ = echo.wait();

			let uie = match UiEvent::try_from(msg.as_str()) {
				Ok(uie) => uie,
				Err(_) => return Ok(()),
			};

			println!("{:#?}", uie);

			Ok(())
		})
		.and_then(|()| {
			println!("Socket received FIN packet and closed connection");
			Ok(())
		})
		.or_else(|err| {
			println!("Socket closed with error: {:?}", err);
			Err(err)
		})
		.then(|result| {
			println!("Socket closed with result: {:?}", result);
			Ok(())
		});

	tokio::spawn(processor);
	Ok(())
}

fn main() -> Result<(), Box<std::error::Error>> {
	let addr = SocketAddr::from(ADDR);
	let socket = TcpListener::bind(&addr)?;

	let done = socket
		.incoming()
		.map_err(|_| ())
		.for_each(move |socket| process_connections(socket));

	tokio::run(done);
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
