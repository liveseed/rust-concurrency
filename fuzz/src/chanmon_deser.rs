// This file is auto-generated by gen_target.sh based on msg_target_template.txt
// To modify it, modify msg_target_template.txt and run gen_target.sh instead.

use bitcoin_hashes::sha256d::Hash as Sha256dHash;

use lightning::util::enforcing_trait_impls::EnforcingChannelKeys;
use lightning::ln::channelmonitor;
use lightning::util::ser::{ReadableArgs, Writer};

use utils::test_logger;

use std::io::Cursor;
use std::sync::Arc;

struct VecWriter(Vec<u8>);
impl Writer for VecWriter {
	fn write_all(&mut self, buf: &[u8]) -> Result<(), ::std::io::Error> {
		self.0.extend_from_slice(buf);
		Ok(())
	}
	fn size_hint(&mut self, size: usize) {
		self.0.reserve_exact(size);
	}
}

#[inline]
pub fn do_test(data: &[u8]) {
	let logger = Arc::new(test_logger::TestLogger::new("".to_owned()));
	if let Ok((latest_block_hash, monitor)) = <(Sha256dHash, channelmonitor::ChannelMonitor<EnforcingChannelKeys>)>::read(&mut Cursor::new(data), logger.clone()) {
		let mut w = VecWriter(Vec::new());
		monitor.write_for_disk(&mut w).unwrap();
		let deserialized_copy = <(Sha256dHash, channelmonitor::ChannelMonitor<EnforcingChannelKeys>)>::read(&mut Cursor::new(&w.0), logger.clone()).unwrap();
		assert!(latest_block_hash == deserialized_copy.0);
		assert!(monitor == deserialized_copy.1);
		w.0.clear();
		monitor.write_for_watchtower(&mut w).unwrap();
	}
}

#[no_mangle]
pub extern "C" fn chanmon_deser_run(data: *const u8, datalen: usize) {
	do_test(unsafe { std::slice::from_raw_parts(data, datalen) });
}
