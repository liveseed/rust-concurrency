use ln::chan_utils::{HTLCOutputInCommitment, TxCreationKeys};
use ln::msgs;
use chain::keysinterface::{ChannelKeys, InMemoryChannelKeys};

use std::cmp;
use std::sync::Mutex;

use bitcoin::blockdata::transaction::Transaction;
use bitcoin::blockdata::script::Script;

use secp256k1;
use secp256k1::key::SecretKey;
use secp256k1::{Secp256k1, Signature};

/// Enforces some rules on ChannelKeys calls. Eventually we will probably want to expose a variant
/// of this which would essentially be what you'd want to run on a hardware wallet.
pub struct EnforcingChannelKeys {
	pub inner: InMemoryChannelKeys,
	commitment_number_obscure_and_last: Mutex<(Option<u64>, u64)>,
}

impl EnforcingChannelKeys {
	pub fn new(inner: InMemoryChannelKeys) -> Self {
		Self {
			inner,
			commitment_number_obscure_and_last: Mutex::new((None, 0)),
		}
	}
}
impl ChannelKeys for EnforcingChannelKeys {
	fn funding_key(&self) -> &SecretKey { self.inner.funding_key() }
	fn revocation_base_key(&self) -> &SecretKey { self.inner.revocation_base_key() }
	fn payment_base_key(&self) -> &SecretKey { self.inner.payment_base_key() }
	fn delayed_payment_base_key(&self) -> &SecretKey { self.inner.delayed_payment_base_key() }
	fn htlc_base_key(&self) -> &SecretKey { self.inner.htlc_base_key() }
	fn commitment_seed(&self) -> &[u8; 32] { self.inner.commitment_seed() }

	fn sign_remote_commitment<T: secp256k1::Signing>(&self, channel_value_satoshis: u64, channel_funding_script: &Script, feerate_per_kw: u64, commitment_tx: &Transaction, keys: &TxCreationKeys, htlcs: &[&HTLCOutputInCommitment], to_self_delay: u16, secp_ctx: &Secp256k1<T>) -> Result<(Signature, Vec<Signature>), ()> {
		if commitment_tx.input.len() != 1 { panic!(); }
		let obscured_commitment_transaction_number = (commitment_tx.lock_time & 0xffffff) as u64 | ((commitment_tx.input[0].sequence as u64 & 0xffffff) << 3*8);

		{
			let mut commitment_data = self.commitment_number_obscure_and_last.lock().unwrap();
			if commitment_data.0.is_none() {
				commitment_data.0 = Some(obscured_commitment_transaction_number ^ commitment_data.1);
			}
			let commitment_number = obscured_commitment_transaction_number ^ commitment_data.0.unwrap();
			assert!(commitment_number == commitment_data.1 || commitment_number == commitment_data.1 + 1);
			commitment_data.1 = cmp::max(commitment_number, commitment_data.1)
		}

		Ok(self.inner.sign_remote_commitment(channel_value_satoshis, channel_funding_script, feerate_per_kw, commitment_tx, keys, htlcs, to_self_delay, secp_ctx).unwrap())
	}

	fn sign_closing_transaction<T: secp256k1::Signing>(&self, channel_value_satoshis: u64, channel_funding_redeemscript: &Script, closing_tx: &Transaction, secp_ctx: &Secp256k1<T>) -> Result<Signature, ()> {
		Ok(self.inner.sign_closing_transaction(channel_value_satoshis, channel_funding_redeemscript, closing_tx, secp_ctx).unwrap())
	}

	fn sign_channel_announcement<T: secp256k1::Signing>(&self, msg: &msgs::UnsignedChannelAnnouncement, secp_ctx: &Secp256k1<T>) -> Result<Signature, ()> {
		self.inner.sign_channel_announcement(msg, secp_ctx)
	}
}

impl_writeable!(EnforcingChannelKeys, 0, {
	inner,
	commitment_number_obscure_and_last
});
