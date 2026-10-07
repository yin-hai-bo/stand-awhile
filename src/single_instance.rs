use std::time::{Duration, Instant};

use windows::{
    Win32::{
        Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, LPARAM, WPARAM},
        System::Threading::CreateMutexW,
        UI::WindowsAndMessaging::{
            AllowSetForegroundWindow, FindWindowW, GetWindowThreadProcessId, PostMessageW, WM_APP,
        },
    },
    core::{PCWSTR, Result, w},
};

pub const MAIN_WINDOW_CLASS: PCWSTR = w!("YHB-StandAwhileWindowClass");
pub const WM_SHOW_EXISTING_INSTANCE: u32 = WM_APP + 3;
const INSTANCE_MUTEX: PCWSTR = w!("Local\\Yinhaibo.StandAwhile");

pub struct SingleInstance(HANDLE);

impl SingleInstance {
    pub fn acquire() -> Result<Option<Self>> {
        Self::acquire_named(INSTANCE_MUTEX)
    }

    fn acquire_named(name: PCWSTR) -> Result<Option<Self>> {
        let handle = unsafe { CreateMutexW(None, false, name)? };
        let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let instance = Self(handle);
        if already_exists { Ok(None) } else { Ok(Some(instance)) }
    }
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

pub fn activate_existing() -> Result<bool> {
    let Some(hwnd) = wait_for_window(MAIN_WINDOW_CLASS, Duration::from_secs(5)) else {
        return Ok(false);
    };
    unsafe {
        let mut process_id = 0;
        GetWindowThreadProcessId(hwnd, Some(&mut process_id));
        // The newly launched process may pass its foreground permission to the existing one.
        let _ = AllowSetForegroundWindow(process_id);
        PostMessageW(Some(hwnd), WM_SHOW_EXISTING_INSTANCE, WPARAM(0), LPARAM(0))?;
    }
    Ok(true)
}

fn wait_for_window(class_name: PCWSTR, timeout: Duration) -> Option<HWND> {
    let started = Instant::now();
    loop {
        if let Ok(hwnd) = unsafe { FindWindowW(class_name, None) } {
            return Some(hwnd);
        }
        let remaining = timeout.checked_sub(started.elapsed())?;
        std::thread::sleep(remaining.min(Duration::from_millis(50)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Child, Command, Stdio},
        sync::{
            Arc, Barrier,
            atomic::{AtomicU32, Ordering},
            mpsc,
        },
    };

    fn unique_mutex() -> Vec<u16> {
        static NEXT_ID: AtomicU32 = AtomicU32::new(0);
        format!(
            "Local\\StandAwhileTest-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        )
        .encode_utf16()
        .chain([0])
        .collect()
    }

    #[test]
    fn concurrent_acquisition_allows_one_instance_and_release_allows_restart() {
        let name = Arc::new(unique_mutex());
        let start = Arc::new(Barrier::new(8));
        let finish = Arc::new(Barrier::new(8));
        let winners = Arc::new(AtomicU32::new(0));
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let (name, start, finish, winners) = (name.clone(), start.clone(), finish.clone(), winners.clone());
                std::thread::spawn(move || {
                    start.wait();
                    let instance = SingleInstance::acquire_named(PCWSTR(name.as_ptr())).unwrap();
                    if instance.is_some() {
                        winners.fetch_add(1, Ordering::Relaxed);
                    }
                    finish.wait();
                    drop(instance);
                })
            })
            .collect();
        for thread in threads {
            thread.join().unwrap();
        }
        assert_eq!(winners.load(Ordering::Relaxed), 1);
        assert!(SingleInstance::acquire_named(PCWSTR(name.as_ptr())).unwrap().is_some());
    }

    // This fixture runs in a child test process, using a test-only mutex name.
    #[test]
    fn subprocess_fixture() {
        let Ok(name) = std::env::var("STAND_AWHILE_TEST_MUTEX") else {
            return;
        };
        let name: Vec<u16> = name.encode_utf16().chain([0]).collect();
        let _instance = SingleInstance::acquire_named(PCWSTR(name.as_ptr())).unwrap().unwrap();
        println!("INSTANCE_READY");
        std::io::stdout().flush().unwrap();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
    }

    struct TestProcess(Child);

    impl Drop for TestProcess {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    #[test]
    fn normal_and_abnormal_process_exit_allow_restart() {
        for terminate in [false, true] {
            let name = unique_mutex();
            let name_text = String::from_utf16(&name[..name.len() - 1]).unwrap();
            let mut child = TestProcess(
                Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "single_instance::tests::subprocess_fixture", "--nocapture"])
                    .env("STAND_AWHILE_TEST_MUTEX", name_text)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::null())
                    .spawn()
                    .unwrap(),
            );
            let stdout = child.0.stdout.take().unwrap();
            let (ready_tx, ready_rx) = mpsc::channel();
            let reader = std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    if line.unwrap().trim() == "INSTANCE_READY" {
                        let _ = ready_tx.send(());
                    }
                }
            });
            ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert!(SingleInstance::acquire_named(PCWSTR(name.as_ptr())).unwrap().is_none());
            if terminate {
                child.0.kill().unwrap();
            } else {
                child.0.stdin.as_mut().unwrap().write_all(b"exit\n").unwrap();
            }
            let status = child.0.wait().unwrap();
            if !terminate {
                assert!(status.success());
            }
            reader.join().unwrap();
            assert!(SingleInstance::acquire_named(PCWSTR(name.as_ptr())).unwrap().is_some());
        }
    }

    #[test]
    fn window_lookup_waits_for_creation_and_times_out_when_missing() {
        use windows::Win32::{
            Foundation::HINSTANCE,
            System::LibraryLoader::GetModuleHandleW,
            UI::WindowsAndMessaging::{CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, WNDCLASSW},
        };
        unsafe extern "system" fn test_proc(
            hwnd: HWND,
            msg: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> windows::Win32::Foundation::LRESULT {
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        let class_name = w!("StandAwhileDelayedInstanceWindowTest");
        let (release_tx, release_rx) = mpsc::channel();
        let worker = std::thread::spawn(move || {
            let instance: HINSTANCE = unsafe { GetModuleHandleW(None).unwrap() }.into();
            let class = WNDCLASSW {
                lpfnWndProc: Some(test_proc),
                hInstance: instance,
                lpszClassName: w!("StandAwhileDelayedInstanceWindowTest"),
                ..Default::default()
            };
            assert_ne!(unsafe { RegisterClassW(&class) }, 0);
            std::thread::sleep(Duration::from_millis(50));
            let hwnd = unsafe {
                CreateWindowExW(
                    Default::default(),
                    class.lpszClassName,
                    w!(""),
                    Default::default(),
                    0,
                    0,
                    100,
                    100,
                    None,
                    None,
                    Some(instance),
                    None,
                )
                .unwrap()
            };
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            unsafe {
                DestroyWindow(hwnd).unwrap();
            }
        });
        assert!(wait_for_window(class_name, Duration::from_secs(2)).is_some());
        release_tx.send(()).unwrap();
        worker.join().unwrap();
        let started = Instant::now();
        assert!(wait_for_window(class_name, Duration::from_millis(30)).is_none());
        assert!(started.elapsed() >= Duration::from_millis(30));
        assert!(started.elapsed() < Duration::from_secs(1));
    }
}
