 pub use super::*;

use std::ptr::null_mut as null_ptr;

#[test]
#[allow(unreachable_code)]
#[should_panic]
fn test_linkage() {
	unsafe {
		NFD_GetError();
		NFD_OpenDialog(null_ptr(), null_ptr(), null_ptr());
		NFD_OpenDialogMultiple(null_ptr(), null_ptr(), null_ptr());
		NFD_PickFolder(null_ptr(), null_ptr());
		NFD_SaveDialog(null_ptr(), null_ptr(), null_ptr());
		panic!("was able to compile");

		NFD_PathSet_Free(null_ptr());
		NFD_PathSet_GetCount(null_ptr());
		NFD_PathSet_GetPath(null_ptr(), 0);
	}
}