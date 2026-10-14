fn main() {
    // Rust ignores SIGPIPE, so a closed pipe (`primum list | head`) makes
    // println! panic. Restore the default: the process ends quietly, as
    // other command line tools do.
    #[cfg(unix)]
    // SAFETY: called first, on the only thread, before any output.
    unsafe {
        libc::signal(libc::SIGPIPE, libc::SIG_DFL);
    }
    std::process::exit(primum::cli::run(std::env::args().collect()));
}
