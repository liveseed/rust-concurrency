//! Lightning exposes sets of supported operations through "feature flags". This module includes
//! types to store those feature flags and query for specific flags.

use std::{cmp, fmt};
use std::result::Result;
use std::marker::PhantomData;

use ln::msgs::DecodeError;
use util::ser::{Readable, Writeable, Writer};

mod sealed { // You should just use the type aliases instead.
	pub struct InitContext {}
	pub struct NodeContext {}
	pub struct ChannelContext {}

	/// An internal trait capturing the various feature context types
	pub trait Context {}
	impl Context for InitContext {}
	impl Context for NodeContext {}
	impl Context for ChannelContext {}

	pub trait DataLossProtect: Context {}
	impl DataLossProtect for InitContext {}
	impl DataLossProtect for NodeContext {}

	pub trait InitialRoutingSync: Context {}
	impl InitialRoutingSync for InitContext {}

	pub trait UpfrontShutdownScript: Context {}
	impl UpfrontShutdownScript for InitContext {}
	impl UpfrontShutdownScript for NodeContext {}

	pub trait VariableLengthOnion: Context {}
	impl VariableLengthOnion for InitContext {}
	impl VariableLengthOnion for NodeContext {}
}

/// Tracks the set of features which a node implements, templated by the context in which it
/// appears.
pub struct Features<T: sealed::Context> {
	/// Note that, for convinience, flags is LITTLE endian (despite being big-endian on the wire)
	flags: Vec<u8>,
	mark: PhantomData<T>,
}

impl<T: sealed::Context> Clone for Features<T> {
	fn clone(&self) -> Self {
		Self {
			flags: self.flags.clone(),
			mark: PhantomData,
		}
	}
}
impl<T: sealed::Context> PartialEq for Features<T> {
	fn eq(&self, o: &Self) -> bool {
		self.flags.eq(&o.flags)
	}
}
impl<T: sealed::Context> fmt::Debug for Features<T> {
	fn fmt(&self, fmt: &mut fmt::Formatter) -> Result<(), fmt::Error> {
		self.flags.fmt(fmt)
	}
}

/// A feature message as it appears in an init message
pub type InitFeatures = Features<sealed::InitContext>;
/// A feature message as it appears in a node_announcement message
pub type NodeFeatures = Features<sealed::NodeContext>;
/// A feature message as it appears in a channel_announcement message
pub type ChannelFeatures = Features<sealed::ChannelContext>;

impl InitFeatures {
	/// Create a Features with the features we support
	pub fn supported() -> InitFeatures {
		InitFeatures {
			flags: vec![2 | 1 << 5, 1 << (9-8)],
			mark: PhantomData,
		}
	}

	/// Writes all features present up to, and including, 13.
	pub(crate) fn write_up_to_13<W: Writer>(&self, w: &mut W) -> Result<(), ::std::io::Error> {
		let len = cmp::min(2, self.flags.len());
		w.size_hint(len + 2);
		(len as u16).write(w)?;
		for i in (0..len).rev() {
			if i == 0 {
				self.flags[i].write(w)?;
			} else {
				// On byte 1, we want up-to-and-including-bit-13, 0-indexed, which is
				// up-to-and-including-bit-5, 0-indexed, on this byte:
				(self.flags[i] & 0b00_11_11_11).write(w)?;
			}
		}
		Ok(())
	}

	/// or's another InitFeatures into this one.
	pub(crate) fn or(mut self, o: InitFeatures) -> InitFeatures {
		let total_feature_len = cmp::max(self.flags.len(), o.flags.len());
		self.flags.resize(total_feature_len, 0u8);
		for (byte, o_byte) in self.flags.iter_mut().zip(o.flags.iter()) {
			*byte |= *o_byte;
		}
		self
	}
}

impl ChannelFeatures {
	/// Create a Features with the features we support
	#[cfg(not(feature = "fuzztarget"))]
	pub(crate) fn supported() -> ChannelFeatures {
		ChannelFeatures {
			flags: Vec::new(),
			mark: PhantomData,
		}
	}
	#[cfg(feature = "fuzztarget")]
	pub fn supported() -> ChannelFeatures {
		ChannelFeatures {
			flags: Vec::new(),
			mark: PhantomData,
		}
	}

	/// Takes the flags that we know how to interpret in an init-context features that are also
	/// relevant in a channel-context features and creates a channel-context features from them.
	pub(crate) fn with_known_relevant_init_flags(_init_ctx: &InitFeatures) -> Self {
		// There are currently no channel flags defined that we understand.
		Self { flags: Vec::new(), mark: PhantomData, }
	}
}

impl NodeFeatures {
	/// Create a Features with the features we support
	#[cfg(not(feature = "fuzztarget"))]
	pub(crate) fn supported() -> NodeFeatures {
		NodeFeatures {
			flags: vec![2 | 1 << 5, 1 << (9-8)],
			mark: PhantomData,
		}
	}
	#[cfg(feature = "fuzztarget")]
	pub fn supported() -> NodeFeatures {
		NodeFeatures {
			flags: vec![2 | 1 << 5, 1 << (9-8)],
			mark: PhantomData,
		}
	}

	/// Takes the flags that we know how to interpret in an init-context features that are also
	/// relevant in a node-context features and creates a node-context features from them.
	/// Be sure to blank out features that are unknown to us.
	pub(crate) fn with_known_relevant_init_flags(init_ctx: &InitFeatures) -> Self {
		let mut flags = Vec::new();
		for (i, feature_byte)in init_ctx.flags.iter().enumerate() {
			match i {
				// Blank out initial_routing_sync (feature bits 2/3), gossip_queries (6/7),
				// gossip_queries_ex (10/11), option_static_remotekey (12/13), and
				// payment_secret (14/15)
				0 => flags.push(feature_byte & 0b00110011),
				1 => flags.push(feature_byte & 0b00000011),
				_ => (),
			}
		}
		Self { flags, mark: PhantomData, }
	}
}

impl<T: sealed::Context> Features<T> {
	/// Create a blank Features with no features set
	pub fn empty() -> Features<T> {
		Features {
			flags: Vec::new(),
			mark: PhantomData,
		}
	}

	#[cfg(test)]
	/// Create a Features given a set of flags, in LE.
	pub fn from_le_bytes(flags: Vec<u8>) -> Features<T> {
		Features {
			flags,
			mark: PhantomData,
		}
	}

	#[cfg(test)]
	/// Gets the underlying flags set, in LE.
	pub fn le_flags(&self) -> &Vec<u8> {
		&self.flags
	}

	pub(crate) fn requires_unknown_bits(&self) -> bool {
		self.flags.iter().enumerate().any(|(idx, &byte)| {
			(match idx {
				// Unknown bits are even bits which we don't understand, we list ones which we do
				// here:
				// unknown, upfront_shutdown_script, unknown (actually initial_routing_sync, but it
				// is only valid as an optional feature), and data_loss_protect:
				0 => (byte & 0b01000100),
				// unknown, unknown, unknown, var_onion_optin:
				1 => (byte & 0b01010100),
				// fallback, all even bits set:
				_ => (byte & 0b01010101),
			}) != 0
		})
	}

	pub(crate) fn supports_unknown_bits(&self) -> bool {
		self.flags.iter().enumerate().any(|(idx, &byte)| {
			(match idx {
				// unknown, upfront_shutdown_script, initial_routing_sync (is only valid as an
				// optional feature), and data_loss_protect:
				0 => (byte & 0b11000100),
				// unknown, unknown, unknown, var_onion_optin:
				1 => (byte & 0b11111100),
				_ => byte,
			}) != 0
		})
	}

	/// The number of bytes required to represent the feature flags present. This does not include
	/// the length bytes which are included in the serialized form.
	pub(crate) fn byte_count(&self) -> usize {
		self.flags.len()
	}

	#[cfg(test)]
	pub(crate) fn set_require_unknown_bits(&mut self) {
		let newlen = cmp::max(2, self.flags.len());
		self.flags.resize(newlen, 0u8);
		self.flags[1] |= 0x40;
	}

	#[cfg(test)]
	pub(crate) fn clear_require_unknown_bits(&mut self) {
		let newlen = cmp::max(2, self.flags.len());
		self.flags.resize(newlen, 0u8);
		self.flags[1] &= !0x40;
		if self.flags.len() == 2 && self.flags[1] == 0 {
			self.flags.resize(1, 0u8);
		}
	}
}

impl<T: sealed::DataLossProtect> Features<T> {
	pub(crate) fn supports_data_loss_protect(&self) -> bool {
		self.flags.len() > 0 && (self.flags[0] & 3) != 0
	}
}

impl<T: sealed::UpfrontShutdownScript> Features<T> {
	pub(crate) fn supports_upfront_shutdown_script(&self) -> bool {
		self.flags.len() > 0 && (self.flags[0] & (3 << 4)) != 0
	}
	#[cfg(test)]
	pub(crate) fn unset_upfront_shutdown_script(&mut self) {
		self.flags[0] ^= 1 << 5;
	}
}

impl<T: sealed::VariableLengthOnion> Features<T> {
	pub(crate) fn supports_variable_length_onion(&self) -> bool {
		self.flags.len() > 1 && (self.flags[1] & 3) != 0
	}
}

impl<T: sealed::InitialRoutingSync> Features<T> {
	pub(crate) fn initial_routing_sync(&self) -> bool {
		self.flags.len() > 0 && (self.flags[0] & (1 << 3)) != 0
	}
	pub(crate) fn set_initial_routing_sync(&mut self) {
		if self.flags.len() == 0 {
			self.flags.resize(1, 1 << 3);
		} else {
			self.flags[0] |= 1 << 3;
		}
	}
}

impl<T: sealed::Context> Writeable for Features<T> {
	fn write<W: Writer>(&self, w: &mut W) -> Result<(), ::std::io::Error> {
		w.size_hint(self.flags.len() + 2);
		(self.flags.len() as u16).write(w)?;
		for f in self.flags.iter().rev() { // Swap back to big-endian
			f.write(w)?;
		}
		Ok(())
	}
}

impl<T: sealed::Context> Readable for Features<T> {
	fn read<R: ::std::io::Read>(r: &mut R) -> Result<Self, DecodeError> {
		let mut flags: Vec<u8> = Readable::read(r)?;
		flags.reverse(); // Swap to little-endian
		Ok(Self {
			flags,
			mark: PhantomData,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::{ChannelFeatures, InitFeatures, NodeFeatures, Features};

	#[test]
	fn sanity_test_our_features() {
		assert!(!ChannelFeatures::supported().requires_unknown_bits());
		assert!(!ChannelFeatures::supported().supports_unknown_bits());
		assert!(!InitFeatures::supported().requires_unknown_bits());
		assert!(!InitFeatures::supported().supports_unknown_bits());
		assert!(!NodeFeatures::supported().requires_unknown_bits());
		assert!(!NodeFeatures::supported().supports_unknown_bits());

		assert!(InitFeatures::supported().supports_upfront_shutdown_script());
		assert!(NodeFeatures::supported().supports_upfront_shutdown_script());

		assert!(InitFeatures::supported().supports_data_loss_protect());
		assert!(NodeFeatures::supported().supports_data_loss_protect());

		assert!(InitFeatures::supported().supports_variable_length_onion());
		assert!(NodeFeatures::supported().supports_variable_length_onion());

		let mut init_features = InitFeatures::supported();
		init_features.set_initial_routing_sync();
		assert!(!init_features.requires_unknown_bits());
		assert!(!init_features.supports_unknown_bits());
	}

	#[test]
	fn sanity_test_unkown_bits_testing() {
		let mut features = ChannelFeatures::supported();
		features.set_require_unknown_bits();
		assert!(features.requires_unknown_bits());
		features.clear_require_unknown_bits();
		assert!(!features.requires_unknown_bits());
	}

	#[test]
	fn test_node_with_known_relevant_init_flags() {
		// Create an InitFeatures with initial_routing_sync supported.
		let mut init_features = InitFeatures::supported();
		init_features.set_initial_routing_sync();

		// Attempt to pull out non-node-context feature flags from these InitFeatures.
		let res = NodeFeatures::with_known_relevant_init_flags(&init_features);

		{
			// Check that the flags are as expected: optional_data_loss_protect,
			// option_upfront_shutdown_script, and var_onion_optin set.
			assert_eq!(res.flags[0], 0b00100010);
			assert_eq!(res.flags[1], 0b00000010);
			assert_eq!(res.flags.len(), 2);
		}

		// Check that the initial_routing_sync feature was correctly blanked out.
		let new_features: InitFeatures = Features::from_le_bytes(res.flags);
		assert!(!new_features.initial_routing_sync());
	}
}
