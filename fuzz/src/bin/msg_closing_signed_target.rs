// This file is Copyright its original authors, visible in version control
// history.
//
// This file is licensed under the Apache License, Version 2.0 <LICENSE-APACHE
// or http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your option.
// You may not use this file except in accordance with one or both of these
// licenses.

// This file is auto-generated by gen_target.sh based on target_template.txt
// To modify it, modify target_template.txt and run gen_target.sh instead.

#![cfg_attr(feature = "libfuzzer_fuzz", no_main)]

extern crate lightning_fuzz;
use lightning_fuzz::msg_targets::msg_closing_signed::*;

#[cfg(feature = "afl")]
#[macro_use] extern crate afl;
#[cfg(feature = "afl")]
fn main() {
	fuzz!(|data| {
		msg_closing_signed_run(data.as_ptr(), data.len());
	});
}

#[cfg(feature = "honggfuzz")]
#[macro_use] extern crate honggfuzz;
#[cfg(feature = "honggfuzz")]
fn main() {
	loop {
		fuzz!(|data| {
			msg_closing_signed_run(data.as_ptr(), data.len());
		});
	}
}

#[cfg(feature = "libfuzzer_fuzz")]
#[macro_use] extern crate libfuzzer_sys;
#[cfg(feature = "libfuzzer_fuzz")]
fuzz_target!(|data: &[u8]| {
	msg_closing_signed_run(data.as_ptr(), data.len());
});

#[cfg(feature = "stdin_fuzz")]
fn main() {
	use std::io::Read;

	let mut data = Vec::with_capacity(8192);
	std::io::stdin().read_to_end(&mut data).unwrap();
	msg_closing_signed_run(data.as_ptr(), data.len());
}

#[test]
fn run_test_cases() {
	use std::fs;
	use std::io::Read;
	use lightning_fuzz::utils::test_logger::StringBuffer;

	use std::sync::{atomic, Arc};
	{
		let data: Vec<u8> = vec![0];
		msg_closing_signed_run(data.as_ptr(), data.len());
	}
	let mut threads = Vec::new();
	let threads_running = Arc::new(atomic::AtomicUsize::new(0));
	if let Ok(tests) = fs::read_dir("test_cases/msg_closing_signed") {
		for test in tests {
			let mut data: Vec<u8> = Vec::new();
			let path = test.unwrap().path();
			fs::File::open(&path).unwrap().read_to_end(&mut data).unwrap();
			threads_running.fetch_add(1, atomic::Ordering::AcqRel);

			let thread_count_ref = Arc::clone(&threads_running);
			let main_thread_ref = std::thread::current();
			threads.push((path.file_name().unwrap().to_str().unwrap().to_string(),
				std::thread::spawn(move || {
					let string_logger = StringBuffer::new();

					let panic_logger = string_logger.clone();
					let res = if ::std::panic::catch_unwind(move || {
						msg_closing_signed_test(&data, panic_logger);
					}).is_err() {
						Some(string_logger.into_string())
					} else { None };
					thread_count_ref.fetch_sub(1, atomic::Ordering::AcqRel);
					main_thread_ref.unpark();
					res
				})
			));
			while threads_running.load(atomic::Ordering::Acquire) > 32 {
				std::thread::park();
			}
		}
	}
	for (test, thread) in threads.drain(..) {
		if let Some(output) = thread.join().unwrap() {
			println!("Output of {}:\n{}", test, output);
			panic!();
		}
	}
}
