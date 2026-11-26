//! The WebAssembly interface, with no binding generator: JavaScript asks for an input buffer,
//! writes a UTF-8 request into it, calls `pf_call`, and reads the response from the output
//! buffer. Everything runs on the tab's single thread.
use std::cell::RefCell;

thread_local! {
    static INPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    static OUTPUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
}

/// Returns a buffer of `len` bytes for the next request.
#[no_mangle]
pub extern "C" fn pf_input(len: usize) -> *mut u8 {
    INPUT.with(|b| {
        let mut b = b.borrow_mut();
        b.clear();
        b.resize(len, 0);
        b.as_mut_ptr()
    })
}

/// Runs the request in the input buffer; returns the status and fills the output buffer.
#[no_mangle]
pub extern "C" fn pf_call() -> i32 {
    let request = INPUT.with(|b| String::from_utf8_lossy(&b.borrow()).into_owned());
    let (code, out) = crate::api::call(&request);
    OUTPUT.with(|b| *b.borrow_mut() = out.into_bytes());
    code
}

#[no_mangle]
pub extern "C" fn pf_output_ptr() -> *const u8 {
    OUTPUT.with(|b| b.borrow().as_ptr())
}

#[no_mangle]
pub extern "C" fn pf_output_len() -> usize {
    OUTPUT.with(|b| b.borrow().len())
}
