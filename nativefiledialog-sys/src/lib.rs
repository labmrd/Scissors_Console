#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

#[cfg(test)]
mod test;

/* automatically generated by rust-bindgen */

pub type nfdchar_t = ::std::os::raw::c_char;
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct nfdpathset_t {
	pub buf: *mut nfdchar_t,
	pub indices: *mut usize,
	pub count: usize,
}

impl Default for nfdpathset_t {
	fn default() -> Self {
		unsafe { ::std::mem::zeroed() }
	}
}
#[repr(u32)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum nfdresult_t {
	NFD_ERROR = 0,
	NFD_OKAY = 1,
	NFD_CANCEL = 2,
}
extern "C" {
	pub fn NFD_OpenDialog(
		filterList: *const nfdchar_t,
		defaultPath: *const nfdchar_t,
		outPath: *mut *mut nfdchar_t,
	) -> nfdresult_t;
}
extern "C" {
	pub fn NFD_OpenDialogMultiple(
		filterList: *const nfdchar_t,
		defaultPath: *const nfdchar_t,
		outPaths: *mut nfdpathset_t,
	) -> nfdresult_t;
}
extern "C" {
	pub fn NFD_SaveDialog(
		filterList: *const nfdchar_t,
		defaultPath: *const nfdchar_t,
		outPath: *mut *mut nfdchar_t,
	) -> nfdresult_t;
}
extern "C" {
	pub fn NFD_PickFolder(
		defaultPath: *const nfdchar_t,
		outPath: *mut *mut nfdchar_t,
	) -> nfdresult_t;
}
extern "C" {
	pub fn NFD_GetError() -> *const ::std::os::raw::c_char;
}
extern "C" {
	pub fn NFD_PathSet_GetCount(pathSet: *const nfdpathset_t) -> usize;
}
extern "C" {
	pub fn NFD_PathSet_GetPath(pathSet: *const nfdpathset_t, index: usize) -> *mut nfdchar_t;
}
extern "C" {
	pub fn NFD_PathSet_Free(pathSet: *mut nfdpathset_t);
}