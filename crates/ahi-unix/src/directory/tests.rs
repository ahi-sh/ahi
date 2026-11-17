use super::*;
use std::process::Command;
use std::time::{Duration, Instant};

#[test]
fn fifo_and_descriptor_lifetime_checks_are_bounded() {
    // Isolate descriptor counts from parallel tests, and bound a FIFO regression
    // that would otherwise block forever before metadata can be inspected.
    let mut child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "directory::tests::descriptor_child",
            "--ignored",
            "--nocapture",
        ])
        .env("AHI_DIRECTORY_TEST_CHILD", "1")
        .spawn()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(status) = child.try_wait().unwrap() {
            assert!(status.success(), "descriptor checks failed: {status}");
            break;
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("descriptor checks exceeded 10 seconds (possible blocking FIFO open)");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
#[ignore = "run by bounded parent in an isolated process"]
fn descriptor_child() {
    if std::env::var("AHI_DIRECTORY_TEST_CHILD").as_deref() != Ok("1") {
        return;
    }
    let root = std::env::temp_dir().join(format!("ahi-directory-{}", std::process::id()));
    std::fs::create_dir(&root).unwrap();
    std::fs::create_dir(root.join("nested")).unwrap();
    std::fs::write(root.join("nested/file"), "contents").unwrap();
    let fifo = CString::new(root.join("fifo").as_os_str().as_bytes()).unwrap();
    #[cfg(target_os = "macos")]
    type Mode = u16;
    #[cfg(target_os = "linux")]
    type Mode = u32;
    extern "C" {
        fn mkfifo(path: *const std::ffi::c_char, mode: Mode) -> i32;
    }
    // SAFETY: fifo is NUL-terminated and lives through the call. Mode uses
    // the platform's mode_t width and grants owner-only read/write access.
    assert_eq!(unsafe { mkfifo(fifo.as_ptr(), 0o600) }, 0);
    let descriptors = || {
        #[cfg(target_os = "linux")]
        let path = "/proc/self/fd";
        #[cfg(target_os = "macos")]
        let path = "/dev/fd";
        std::fs::read_dir(path).unwrap().count()
    };
    let before = descriptors();
    {
        let directory = Directory::open(&root).unwrap();
        let fifo = directory.open_file(Path::new("fifo")).unwrap();
        assert!(!fifo.metadata().unwrap().is_file());
        drop(fifo);
        for _ in 0..128 {
            let file = directory.open_file(Path::new("nested/file")).unwrap();
            assert_eq!(file.metadata().unwrap().len(), 8);
            assert!(directory
                .open_file(Path::new("nested/missing/file"))
                .is_err());
        }
    }
    assert_eq!(
        descriptors(),
        before,
        "success and error paths must close their descriptors"
    );
    std::fs::remove_dir_all(root).unwrap();
}
