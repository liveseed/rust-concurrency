// This file is Copyright its original authors, visible in version control
// history.
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

//! High level lightning structs and impls live here.
//!
//! You probably want to create a channelmanager::ChannelManager, and a routing::NetGraphMsgHandler first.
//! Then, you probably want to pass them both on to a peer_handler::PeerManager and use that to
//! create/manage connections and call get_and_clear_pending_events after each action, handling
//! them appropriately.
//!
//! When you want to open/close a channel or send a payment, call into your ChannelManager and when
//! you want to learn things about the network topology (eg get a route for sending a payment),
//! call into your NetGraphMsgHandler.

#[cfg(any(test, feature = "_test_utils"))]
#[macro_use]
pub mod functional_test_utils;

pub mod channelmanager;
pub mod msgs;
pub mod peer_handler;
pub mod chan_utils;
pub mod features;
pub mod script;

#[cfg(feature = "fuzztarget")]
pub mod peer_channel_encryptor;
#[cfg(not(feature = "fuzztarget"))]
pub(crate) mod peer_channel_encryptor;

mod channel;
mod onion_utils;
mod wire;

// Older rustc (which we support) refuses to let us call the get_payment_preimage_hash!() macro
// without the node parameter being mut. This is incorrect, and thus newer rustcs will complain
// about an unnecessary mut. Thus, we silence the unused_mut warning in two test modules below.

#[cfg(test)]
#[allow(unused_mut)]
mod functional_tests;
#[cfg(test)]
#[allow(unused_mut)]
mod chanmon_update_fail_tests;
#[cfg(test)]
mod reorg_tests;
#[cfg(test)]
#[allow(unused_mut)]
mod onion_route_tests;

pub use self::peer_channel_encryptor::LN_MAX_MSG_LEN;

/// payment_hash type, use to cross-lock hop
/// (C-not exported) as we just use [u8; 32] directly
#[derive(Hash, Copy, Clone, PartialEq, Eq, Debug)]
pub struct PaymentHash(pub [u8;32]);
/// payment_preimage type, use to route payment between hop
/// (C-not exported) as we just use [u8; 32] directly
#[derive(Hash, Copy, Clone, PartialEq, Eq, Debug)]
pub struct PaymentPreimage(pub [u8;32]);
/// payment_secret type, use to authenticate sender to the receiver and tie MPP HTLCs together
/// (C-not exported) as we just use [u8; 32] directly
#[derive(Hash, Copy, Clone, PartialEq, Eq, Debug)]
pub struct PaymentSecret(pub [u8;32]);

use prelude::*;
use bitcoin::bech32;
use bitcoin::bech32::{Base32Len, FromBase32, ToBase32, WriteBase32, u5};

impl FromBase32 for PaymentSecret {
	type Err = bech32::Error;

	fn from_base32(field_data: &[u5]) -> Result<PaymentSecret, bech32::Error> {
		if field_data.len() != 52 {
			return Err(bech32::Error::InvalidLength)
		} else {
			let data_bytes = Vec::<u8>::from_base32(field_data)?;
			let mut payment_secret = [0; 32];
			payment_secret.copy_from_slice(&data_bytes);
			Ok(PaymentSecret(payment_secret))
		}
	}
}

impl ToBase32 for PaymentSecret {
	fn write_base32<W: WriteBase32>(&self, writer: &mut W) -> Result<(), <W as WriteBase32>::Err> {
		(&self.0[..]).write_base32(writer)
	}
}

impl Base32Len for PaymentSecret {
	fn base32_len(&self) -> usize {
		52
	}
}
