// This file is auto-generated by gen_target.sh based on msg_target_template.txt
// To modify it, modify msg_target_template.txt and run gen_target.sh instead.

extern crate lightning;

use lightning::ln::msgs;
use lightning::util::reset_rng_state;

use lightning::ln::msgs::{MsgEncodable, MsgDecodable};

mod utils;

#[inline]
pub fn do_test(data: &[u8]) {
	reset_rng_state();
	test_msg!(msgs::UpdateFailHTLC, data);
}

#[cfg(feature = "afl")]
extern crate afl;
#[cfg(feature = "afl")]
fn main() {
	afl::read_stdio_bytes(|data| {
		do_test(&data);
	});
}

#[cfg(feature = "honggfuzz")]
#[macro_use] extern crate honggfuzz;
#[cfg(feature = "honggfuzz")]
fn main() {
	loop {
		fuzz!(|data| {
			do_test(data);
		});
	}
}

#[cfg(test)]
mod tests {
	use utils::extend_vec_from_hex;
	#[test]
	fn duplicate_crash() {
		let mut a = Vec::new();
		extend_vec_from_hex("00", &mut a);
		super::do_test(&a);
	}
}
