#![feature(try_from)]

#[macro_use]
extern crate serde_derive;

use std::io::Write;

#[derive(Serialize, Deserialize, Debug)]
pub enum UiEvent {
	StartPressed(String),
	StopPressed,
}

impl UiEvent {
	pub fn send<W: Write>(self, mut net_handle: &mut W) {
		match serde_json::to_writer(&mut net_handle, &self) {
			Ok(_) => (),
			Err(_) => log::error!("Could not send event ({:#?})", &self),
		}

		let _ = net_handle.write_all(b"\n");
	}
}

impl std::convert::TryFrom<&[u8]> for UiEvent {
	type Error = ();

	fn try_from(json: &[u8]) -> Result<Self, Self::Error> {
		serde_json::from_slice::<UiEvent>(&json).map_err(|_| ())
	}
}

impl std::convert::TryFrom<&str> for UiEvent {
	type Error = ();

	fn try_from(json: &str) -> Result<Self, Self::Error> {
		serde_json::from_str::<UiEvent>(&json).map_err(|_| ())
	}
}
