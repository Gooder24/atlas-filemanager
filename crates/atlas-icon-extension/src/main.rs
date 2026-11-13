#[cfg(windows)]
mod imp;

#[cfg(windows)]
fn main() { imp::run(); }

#[cfg(not(windows))]
fn main() {
    eprintln!("atlas-icon-extension is a Windows-only component");
    std::process::exit(1);
}