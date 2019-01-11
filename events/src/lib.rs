#![feature(try_from)]

#[macro_use]
extern crate serde_derive;

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

#[derive(Serialize, Deserialize, Debug)]
pub enum Server {
	CollectionStarted,
	CollectionEnded,
	Msg(String),
}

impl_try_from!(Client, Server);

impl Event<'_> for Server {}
