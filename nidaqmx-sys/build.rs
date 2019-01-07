
fn main() {
	// Tell cargo to link to nidaqmx
	println!(r"cargo:rustc-link-search=/usr/lib/x86_64-linux-gnu/");
	println!(r"cargo:rustc-link-lib=nidaqmx");
}