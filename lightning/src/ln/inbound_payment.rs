// This file is Copyright its original authors, visible in version control
// history.
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

//! Utilities to generate inbound payment information in service of invoice creation.

use alloc::string::ToString;
use bitcoin::hashes::{Hash, HashEngine};
use bitcoin::hashes::cmp::fixed_time_eq;
use bitcoin::hashes::hmac::{Hmac, HmacEngine};
use bitcoin::hashes::sha256::Hash as Sha256;
use chain::keysinterface::{KeyMaterial, KeysInterface, Sign};
use ln::{PaymentHash, PaymentPreimage, PaymentSecret};
use ln::msgs;
use ln::msgs::MAX_VALUE_MSAT;
use util::chacha20::ChaCha20;
use util::crypto::hkdf_extract_expand_thrice;
use util::errors::APIError;
use util::logger::Logger;

use core::convert::TryInto;
use core::ops::Deref;

const IV_LEN: usize = 16;
const METADATA_LEN: usize = 16;
const METADATA_KEY_LEN: usize = 32;
const AMT_MSAT_LEN: usize = 8;
// Used to shift the payment type bits to take up the top 3 bits of the metadata bytes, or to
// retrieve said payment type bits.
const METHOD_TYPE_OFFSET: usize = 5;

/// A set of keys that were HKDF-expanded from an initial call to
/// [`KeysInterface::get_inbound_payment_key_material`].
///
/// [`KeysInterface::get_inbound_payment_key_material`]: crate::chain::keysinterface::KeysInterface::get_inbound_payment_key_material
pub(super) struct ExpandedKey {
	/// The key used to encrypt the bytes containing the payment metadata (i.e. the amount and
	/// expiry, included for payment verification on decryption).
	metadata_key: [u8; 32],
	/// The key used to authenticate an LDK-provided payment hash and metadata as previously
	/// registered with LDK.
	ldk_pmt_hash_key: [u8; 32],
	/// The key used to authenticate a user-provided payment hash and metadata as previously
	/// registered with LDK.
	user_pmt_hash_key: [u8; 32],
}

impl ExpandedKey {
	pub(super) fn new(key_material: &KeyMaterial) -> ExpandedKey {
		let (metadata_key, ldk_pmt_hash_key, user_pmt_hash_key) =
			hkdf_extract_expand_thrice(b"LDK Inbound Payment Key Expansion", &key_material.0);
		Self {
			metadata_key,
			ldk_pmt_hash_key,
			user_pmt_hash_key,
		}
	}
}

enum Method {
	LdkPaymentHash = 0,
	UserPaymentHash = 1,
}

impl Method {
	fn from_bits(bits: u8) -> Result<Method, u8> {
		match bits {
			bits if bits == Method::LdkPaymentHash as u8 => Ok(Method::LdkPaymentHash),
			bits if bits == Method::UserPaymentHash as u8 => Ok(Method::UserPaymentHash),
			unknown => Err(unknown),
		}
	}
}

pub(super) fn create<Signer: Sign, K: Deref>(keys: &ExpandedKey, min_value_msat: Option<u64>, invoice_expiry_delta_secs: u32, keys_manager: &K, highest_seen_timestamp: u64) -> Result<(PaymentHash, PaymentSecret), ()>
	where K::Target: KeysInterface<Signer = Signer>
{
	let metadata_bytes = construct_metadata_bytes(min_value_msat, Method::LdkPaymentHash, invoice_expiry_delta_secs, highest_seen_timestamp)?;

	let mut iv_bytes = [0 as u8; IV_LEN];
	let rand_bytes = keys_manager.get_secure_random_bytes();
	iv_bytes.copy_from_slice(&rand_bytes[..IV_LEN]);

	let mut hmac = HmacEngine::<Sha256>::new(&keys.ldk_pmt_hash_key);
	hmac.input(&iv_bytes);
	hmac.input(&metadata_bytes);
	let payment_preimage_bytes = Hmac::from_engine(hmac).into_inner();

	let ldk_pmt_hash = PaymentHash(Sha256::hash(&payment_preimage_bytes).into_inner());
	let payment_secret = construct_payment_secret(&iv_bytes, &metadata_bytes, &keys.metadata_key);
	Ok((ldk_pmt_hash, payment_secret))
}

pub(super) fn create_from_hash(keys: &ExpandedKey, min_value_msat: Option<u64>, payment_hash: PaymentHash, invoice_expiry_delta_secs: u32, highest_seen_timestamp: u64) -> Result<PaymentSecret, ()> {
	let metadata_bytes = construct_metadata_bytes(min_value_msat, Method::UserPaymentHash, invoice_expiry_delta_secs, highest_seen_timestamp)?;

	let mut hmac = HmacEngine::<Sha256>::new(&keys.user_pmt_hash_key);
	hmac.input(&metadata_bytes);
	hmac.input(&payment_hash.0);
	let hmac_bytes = Hmac::from_engine(hmac).into_inner();

	let mut iv_bytes = [0 as u8; IV_LEN];
	iv_bytes.copy_from_slice(&hmac_bytes[..IV_LEN]);

	Ok(construct_payment_secret(&iv_bytes, &metadata_bytes, &keys.metadata_key))
}

fn construct_metadata_bytes(min_value_msat: Option<u64>, payment_type: Method, invoice_expiry_delta_secs: u32, highest_seen_timestamp: u64) -> Result<[u8; METADATA_LEN], ()> {
	if min_value_msat.is_some() && min_value_msat.unwrap() > MAX_VALUE_MSAT {
		return Err(());
	}

	let mut min_amt_msat_bytes: [u8; AMT_MSAT_LEN] = match min_value_msat {
		Some(amt) => amt.to_be_bytes(),
		None => [0; AMT_MSAT_LEN],
	};
	min_amt_msat_bytes[0] |= (payment_type as u8) << METHOD_TYPE_OFFSET;

	// We assume that highest_seen_timestamp is pretty close to the current time - it's updated when
	// we receive a new block with the maximum time we've seen in a header. It should never be more
	// than two hours in the future.  Thus, we add two hours here as a buffer to ensure we
	// absolutely never fail a payment too early.
	// Note that we assume that received blocks have reasonably up-to-date timestamps.
	let expiry_bytes = (highest_seen_timestamp + invoice_expiry_delta_secs as u64 + 7200).to_be_bytes();

	let mut metadata_bytes: [u8; METADATA_LEN] = [0; METADATA_LEN];
	metadata_bytes[..AMT_MSAT_LEN].copy_from_slice(&min_amt_msat_bytes);
	metadata_bytes[AMT_MSAT_LEN..].copy_from_slice(&expiry_bytes);

	Ok(metadata_bytes)
}

fn construct_payment_secret(iv_bytes: &[u8; IV_LEN], metadata_bytes: &[u8; METADATA_LEN], metadata_key: &[u8; METADATA_KEY_LEN]) -> PaymentSecret {
	let mut payment_secret_bytes: [u8; 32] = [0; 32];
	let (iv_slice, encrypted_metadata_slice) = payment_secret_bytes.split_at_mut(IV_LEN);
	iv_slice.copy_from_slice(iv_bytes);

	let chacha_block = ChaCha20::get_single_block(metadata_key, iv_bytes);
	for i in 0..METADATA_LEN {
		encrypted_metadata_slice[i] = chacha_block[i] ^ metadata_bytes[i];
	}
	PaymentSecret(payment_secret_bytes)
}

/// Check that an inbound payment's `payment_data` field is sane.
///
/// LDK does not store any data for pending inbound payments. Instead, we construct our payment
/// secret (and, if supplied by LDK, our payment preimage) to include encrypted metadata about the
/// payment.
///
/// The metadata is constructed as:
///   payment method (3 bits) || payment amount (8 bytes - 3 bits) || expiry (8 bytes)
/// and encrypted using a key derived from [`KeysInterface::get_inbound_payment_key_material`].
///
/// Then on payment receipt, we verify in this method that the payment preimage and payment secret
/// match what was constructed.
///
/// [`create_inbound_payment`] and [`create_inbound_payment_for_hash`] are called by the user to
/// construct the payment secret and/or payment hash that this method is verifying. If the former
/// method is called, then the payment method bits mentioned above are represented internally as
/// [`Method::LdkPaymentHash`]. If the latter, [`Method::UserPaymentHash`].
///
/// For the former method, the payment preimage is constructed as an HMAC of payment metadata and
/// random bytes. Because the payment secret is also encoded with these random bytes and metadata
/// (with the metadata encrypted with a block cipher), we're able to authenticate the preimage on
/// payment receipt.
///
/// For the latter, the payment secret instead contains an HMAC of the user-provided payment hash
/// and payment metadata (encrypted with a block cipher), allowing us to authenticate the payment
/// hash and metadata on payment receipt.
///
/// See [`ExpandedKey`] docs for more info on the individual keys used.
///
/// [`KeysInterface::get_inbound_payment_key_material`]: crate::chain::keysinterface::KeysInterface::get_inbound_payment_key_material
/// [`create_inbound_payment`]: crate::ln::channelmanager::ChannelManager::create_inbound_payment
/// [`create_inbound_payment_for_hash`]: crate::ln::channelmanager::ChannelManager::create_inbound_payment_for_hash
pub(super) fn verify<L: Deref>(payment_hash: PaymentHash, payment_data: msgs::FinalOnionHopData, highest_seen_timestamp: u64, keys: &ExpandedKey, logger: &L) -> Result<Option<PaymentPreimage>, ()>
	where L::Target: Logger
{
	let (iv_bytes, metadata_bytes) = decrypt_metadata(payment_data.payment_secret, keys);

	let payment_type_res = Method::from_bits((metadata_bytes[0] & 0b1110_0000) >> METHOD_TYPE_OFFSET);
	let mut amt_msat_bytes = [0; AMT_MSAT_LEN];
	amt_msat_bytes.copy_from_slice(&metadata_bytes[..AMT_MSAT_LEN]);
	// Zero out the bits reserved to indicate the payment type.
	amt_msat_bytes[0] &= 0b00011111;
	let min_amt_msat: u64 = u64::from_be_bytes(amt_msat_bytes.into());
	let expiry = u64::from_be_bytes(metadata_bytes[AMT_MSAT_LEN..].try_into().unwrap());

	// Make sure to check to check the HMAC before doing the other checks below, to mitigate timing
	// attacks.
	let mut payment_preimage = None;
	match payment_type_res {
		Ok(Method::UserPaymentHash) => {
			let mut hmac = HmacEngine::<Sha256>::new(&keys.user_pmt_hash_key);
			hmac.input(&metadata_bytes[..]);
			hmac.input(&payment_hash.0);
			if !fixed_time_eq(&iv_bytes, &Hmac::from_engine(hmac).into_inner().split_at_mut(IV_LEN).0) {
				log_trace!(logger, "Failing HTLC with user-generated payment_hash {}: unexpected payment_secret", log_bytes!(payment_hash.0));
				return Err(())
			}
		},
		Ok(Method::LdkPaymentHash) => {
			match derive_ldk_payment_preimage(payment_hash, &iv_bytes, &metadata_bytes, keys) {
				Ok(preimage) => payment_preimage = Some(preimage),
				Err(bad_preimage_bytes) => {
					log_trace!(logger, "Failing HTLC with payment_hash {} due to mismatching preimage {}", log_bytes!(payment_hash.0), log_bytes!(bad_preimage_bytes));
					return Err(())
				}
			}
		},
		Err(unknown_bits) => {
			log_trace!(logger, "Failing HTLC with payment hash {} due to unknown payment type {}", log_bytes!(payment_hash.0), unknown_bits);
			return Err(());
		}
	}

	if payment_data.total_msat < min_amt_msat {
		log_trace!(logger, "Failing HTLC with payment_hash {} due to total_msat {} being less than the minimum amount of {} msat", log_bytes!(payment_hash.0), payment_data.total_msat, min_amt_msat);
		return Err(())
	}

	if expiry < highest_seen_timestamp {
		log_trace!(logger, "Failing HTLC with payment_hash {}: expired payment", log_bytes!(payment_hash.0));
		return Err(())
	}

	Ok(payment_preimage)
}

pub(super) fn get_payment_preimage(payment_hash: PaymentHash, payment_secret: PaymentSecret, keys: &ExpandedKey) -> Result<PaymentPreimage, APIError> {
	let (iv_bytes, metadata_bytes) = decrypt_metadata(payment_secret, keys);

	match Method::from_bits((metadata_bytes[0] & 0b1110_0000) >> METHOD_TYPE_OFFSET) {
		Ok(Method::LdkPaymentHash) => {
			derive_ldk_payment_preimage(payment_hash, &iv_bytes, &metadata_bytes, keys)
				.map_err(|bad_preimage_bytes| APIError::APIMisuseError {
					err: format!("Payment hash {} did not match decoded preimage {}", log_bytes!(payment_hash.0), log_bytes!(bad_preimage_bytes))
				})
		},
		Ok(Method::UserPaymentHash) => Err(APIError::APIMisuseError {
			err: "Expected payment type to be LdkPaymentHash, instead got UserPaymentHash".to_string()
		}),
		Err(other) => Err(APIError::APIMisuseError { err: format!("Unknown payment type: {}", other) }),
	}
}

fn decrypt_metadata(payment_secret: PaymentSecret, keys: &ExpandedKey) -> ([u8; IV_LEN], [u8; METADATA_LEN]) {
	let mut iv_bytes = [0; IV_LEN];
	let (iv_slice, encrypted_metadata_bytes) = payment_secret.0.split_at(IV_LEN);
	iv_bytes.copy_from_slice(iv_slice);

	let chacha_block = ChaCha20::get_single_block(&keys.metadata_key, &iv_bytes);
	let mut metadata_bytes: [u8; METADATA_LEN] = [0; METADATA_LEN];
	for i in 0..METADATA_LEN {
		metadata_bytes[i] = chacha_block[i] ^ encrypted_metadata_bytes[i];
	}

	(iv_bytes, metadata_bytes)
}

// Errors if the payment preimage doesn't match `payment_hash`. Returns the bad preimage bytes in
// this case.
fn derive_ldk_payment_preimage(payment_hash: PaymentHash, iv_bytes: &[u8; IV_LEN], metadata_bytes: &[u8; METADATA_LEN], keys: &ExpandedKey) -> Result<PaymentPreimage, [u8; 32]> {
	let mut hmac = HmacEngine::<Sha256>::new(&keys.ldk_pmt_hash_key);
	hmac.input(iv_bytes);
	hmac.input(metadata_bytes);
	let decoded_payment_preimage = Hmac::from_engine(hmac).into_inner();
	if !fixed_time_eq(&payment_hash.0, &Sha256::hash(&decoded_payment_preimage).into_inner()) {
		return Err(decoded_payment_preimage);
	}
	return Ok(PaymentPreimage(decoded_payment_preimage))
}
