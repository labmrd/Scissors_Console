
macro_rules! nfd_src {
	($src:expr) => {
		concat!("extern/src/", $src);
	};
}

// const NFD_H_FUNCTIONS: &[&str] = &[
// 	"NFD_OpenDialog", "NFD_OpenDialogMultiple", "NFD_SaveDialog", "NFD_PickFolder", 
// 	"NFD_GetError", "NFD_PathSet_GetCount", "NFD_PathSet_GetPath", "NFD_PathSet_Free"
// ];

const NFD_SRC_FILES: &[&str] = &[
	nfd_src!("nfd_common.c"),

	#[cfg(all(unix, not(target_os = "macos")))]
	nfd_src!("nfd_gtk.c"),

	#[cfg(target_os = "macos")]
	nfd_src!("nfd_cocoa.m"),

	#[cfg(windows)]
	nfd_src!("nfd_win.cpp"),
];

#[cfg(windows)]
const NFD_WIN_LIBS: &[&str] = &["ole32", "uuid", "comctl32", "Shell32"];

fn main() {
	let mut compiler = cc::Build::new();

	compiler.include(nfd_src!("include/")).files(NFD_SRC_FILES).warnings(false);

	add_libs(&mut compiler);

	compiler.compile("nativefiledialog_sys");
}

fn add_libs(_compiler: &mut cc::Build) {

	#[cfg(all(unix, not(target_os = "macos")))]
	{
		let gtk = pkg_config::probe_library("gtk+-3.0")
			.expect("pkg-config could not find libgtk+-3.0");
		
		for inc in gtk.include_paths {
			_compiler.include(inc);
		}
	}

	#[cfg(target_os = "macos")]
	println!("cargo:rustc-link-lib=framework=AppKit");

	#[cfg(windows)]
	for win_lib in NFD_WIN_LIBS {
		println!("cargo:rustc-link-lib={}", win_lib);
	}
}
