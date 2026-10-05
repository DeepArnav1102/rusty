use std::io::Write;

/// Safely captures standard output during the execution of closure `f`.
/// If pipe or dup operations fail, executes `f` directly.
pub fn capture_stdout<F, T>(f: F) -> (T, String)
where
    F: FnOnce() -> T,
{
    #[cfg(unix)]
    unsafe {
        let _ = std::io::stdout().flush();
        let mut pipe_fds = [0; 2];
        if libc::pipe(pipe_fds.as_mut_ptr()) != 0 {
            return (f(), String::new());
        }

        let old_stdout = libc::dup(1);
        if old_stdout < 0 {
            libc::close(pipe_fds[0]);
            libc::close(pipe_fds[1]);
            return (f(), String::new());
        }

        // Redirect stdout (FD 1) to the write-end of pipe
        libc::dup2(pipe_fds[1], 1);
        libc::close(pipe_fds[1]);

        let res = f();

        let _ = std::io::stdout().flush();

        // Restore original stdout
        libc::dup2(old_stdout, 1);
        libc::close(old_stdout);

        // Read available data from read-end of pipe with non-blocking mode
        let flags = libc::fcntl(pipe_fds[0], libc::F_GETFL);
        libc::fcntl(pipe_fds[0], libc::F_SETFL, flags | libc::O_NONBLOCK);

        let mut output = Vec::new();
        let mut buf = [0u8; 2048];
        loop {
            let n = libc::read(
                pipe_fds[0],
                buf.as_mut_ptr() as *mut libc::c_void,
                buf.len(),
            );
            if n > 0 {
                output.extend_from_slice(&buf[..n as usize]);
            } else {
                break;
            }
        }
        libc::close(pipe_fds[0]);

        (res, String::from_utf8_lossy(&output).to_string())
    }

    #[cfg(not(unix))]
    {
        (f(), String::new())
    }
}
