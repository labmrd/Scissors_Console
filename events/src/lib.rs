#![feature(try_from)]

#[macro_use]
extern crate serde_derive;

#[macro_use]
extern crate derive_more;

use std::convert::TryFrom;
use std::fmt::Debug;
use std::io::Write;

use serde::{Deserialize, Serialize};

pub trait Event<'de>: Sized + Debug + Serialize + Deserialize<'de> {
	fn send<W: Write>(self, mut wtr: &mut W) {
		match serde_json::to_writer(&mut wtr, &self) {
			Ok(_) => (),
			Err(_) => log::error!("Could not send event ({:#?})", &self),
		}

		let _ = wtr.write_all(b"\n");
	}

	fn as_str(self) -> Option<String> {
		serde_json::to_string(&self).ok()
	}
}

macro_rules! impl_try_from {
	( $($t:ty),+ ) => {
		$(
			impl TryFrom<&[u8]> for $t {
				type Error = ();

				fn try_from(json: &[u8]) -> Result<Self, Self::Error> {
					serde_json::from_slice::<$t>(&json).map_err(|_| ())
				}
			}

			impl TryFrom<&str> for $t {
				type Error = ();

				fn try_from(json: &str) -> Result<Self, Self::Error> {
					serde_json::from_str::<$t>(&json).map_err(|_| ())
				}
			}
		)+
	};
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Client {
	StartPressed(String),
	StopPressed,
}

impl Event<'_> for Client {}

#[derive(Serialize, Deserialize, Debug, Display)]
pub enum Server {
	#[display(fmt = "Data collection started")]
	CollectionStarted,
	#[display(fmt = "Data collection ended")]
	CollectionEnded,
	Msg(String),
	#[display(fmt = "Datapoint: {},{}", _0, _1)]
	DataPoint(f64, f64)
}

impl_try_from!(Client, Server);

impl Event<'_> for Server {}

pub fn process<'a, Ev>(json: &'a str) -> impl Iterator<Item = Option<Ev>> + 'a
where
	Ev: TryFrom<&'a str>
{
	str::lines(json).map(|json| TryFrom::try_from(json).ok())
}