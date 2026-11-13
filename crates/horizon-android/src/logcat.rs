//! stdout and stderr to logcat. An app's are `/dev/null` on Android, and the game
//! prints its log lines on stderr (as well as into `logs/latest.log` in its home), so
//! `adb logcat -s horizon` shows what the game says as it runs.
use std::ffi::{CString, c_char, c_int};
use std::io::{BufRead, BufReader};
use std::os::fd::FromRawFd;

const ANDROID_LOG_INFO: c_int = 4;

#[link(name = "log")]
unsafe extern "C" {
    fn __android_log_write(prio: c_int, tag: *const c_char, text: *const c_char) -> c_int;
}

/// One line to logcat under `tag`.
fn write(tag: &CString, line: &str) {
    let Ok(text) = CString::new(line.replace('\0', "")) else {
        return;
    };
    // SAFETY: both are valid NUL-terminated strings for the length of the call.
    unsafe {
        __android_log_write(ANDROID_LOG_INFO, tag.as_ptr(), text.as_ptr());
    }
}

/// Points stdout and stderr at a pipe whose lines a thread writes to logcat.
pub(crate) fn forward_stdio(tag: &str) {
    let tag = CString::new(tag).unwrap_or_else(|_| c"horizon".to_owned());
    let mut fds = [0 as c_int; 2];
    // SAFETY: `fds` holds two ints for pipe(2) to fill; dup2(2) takes any open fds.
    let read = unsafe {
        if libc::pipe(fds.as_mut_ptr()) != 0 {
            write(
                &tag,
                "Horizon: no pipe for stdout and stderr; logcat stays quiet",
            );
            return;
        }
        libc::dup2(fds[1], libc::STDOUT_FILENO);
        libc::dup2(fds[1], libc::STDERR_FILENO);
        libc::close(fds[1]);
        // SAFETY: the read end is ours alone from here on.
        std::fs::File::from_raw_fd(fds[0])
    };
    let spawned = std::thread::Builder::new()
        .name("logcat".into())
        .spawn(move || {
            for line in BufReader::new(read).lines() {
                match line {
                    Ok(line) => write(&tag, &line),
                    Err(_) => break,
                }
            }
        });
    if spawned.is_err() {
        // Nobody would drain the pipe: a full one would block every print.
        // SAFETY: putting /dev/null back on the two fds the game prints to.
        unsafe {
            let null = libc::open(c"/dev/null".as_ptr(), libc::O_WRONLY);
            libc::dup2(null, libc::STDOUT_FILENO);
            libc::dup2(null, libc::STDERR_FILENO);
        }
    }
}
