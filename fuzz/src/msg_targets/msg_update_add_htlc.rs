// This file is auto-generated by gen_target.sh based on msg_target_template.txt
// To modify it, modify msg_target_template.txt and run gen_target.sh instead.

use lightning::ln::msgs;

use msg_targets::utils::VecWriter;
use utils::test_logger;

#[inline]
pub fn msg_update_add_htlc_test<Out: test_logger::Output>(data: &[u8], _out: Out) {
	test_msg_hole!(msgs::UpdateAddHTLC, data, 85, 33);
}

#[no_mangle]
pub extern "C" fn msg_update_add_htlc_run(data: *const u8, datalen: usize) {
	let data = unsafe { std::slice::from_raw_parts(data, datalen) };
	test_msg_hole!(msgs::UpdateAddHTLC, data, 85, 33);
}
