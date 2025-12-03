use std::{
    ops::Deref,
    ptr::{null, null_mut},
    rc::Rc,
    sync::{
        Arc, LazyLock, Mutex,
        mpsc::{Receiver, Sender},
    },
    thread::JoinHandle,
};

use windows_sys::Win32::{
    Devices::Usb::GUID_DEVINTERFACE_USB_DEVICE,
    Foundation::{GetLastError, HWND, LPARAM, LRESULT, WPARAM},
    System::LibraryLoader::GetModuleHandleW,
    UI::WindowsAndMessaging::{
        CreateWindowExW, DBT_DEVICEARRIVAL, DBT_DEVICEREMOVECOMPLETE, DBT_DEVTYP_DEVICEINTERFACE,
        DEV_BROADCAST_DEVICEINTERFACE_W, DEVICE_NOTIFY_WINDOW_HANDLE, DefWindowProcW,
        DestroyWindow, DispatchMessageW, GetMessageW, HDEVNOTIFY, HWND_MESSAGE, RegisterClassW,
        RegisterDeviceNotificationW, TranslateMessage, UnregisterClassW,
        UnregisterDeviceNotification, WM_DEVICECHANGE, WNDCLASSW,
    },
};

use crate::error::{PollEventError, Win32Error};

static EVENT_SENDER: LazyLock<Mutex<Option<Sender<UsbConnectionEvent>>>> =
    LazyLock::new(|| Mutex::new(None));

fn get_device_id(dev_brodcast: *const DEV_BROADCAST_DEVICEINTERFACE_W) -> String {
    unsafe {
        let dbcc_name_ptr = (*dev_brodcast).dbcc_name.as_ptr();

        let mut len = 0;
        while *dbcc_name_ptr.add(len) != 0 {
            len += 1;
        }
        let slice = std::slice::from_raw_parts(dbcc_name_ptr, len);

        String::from_utf16_lossy(slice)
    }
}

fn handle_device_arrival(dev_brodcast: *const DEV_BROADCAST_DEVICEINTERFACE_W) {
    let dev_type = unsafe { (*dev_brodcast).dbcc_devicetype };
    if dev_type != DBT_DEVTYP_DEVICEINTERFACE {
        return;
    }

    let device_id = get_device_id(dev_brodcast).into();

    let mutex_guard = EVENT_SENDER.lock();
    if mutex_guard.is_err() {
        return;
    }

    if let Some(sender) = &*mutex_guard.unwrap() {
        let _ = sender.send(UsbConnectionEvent::Connected(device_id));
    }
}

fn handle_device_removal(dev_brodcast: *const DEV_BROADCAST_DEVICEINTERFACE_W) {
    let dev_type = unsafe { (*dev_brodcast).dbcc_devicetype };
    if dev_type != DBT_DEVTYP_DEVICEINTERFACE {
        return;
    }

    let device_id = get_device_id(dev_brodcast).into();

    let mutex_guard = EVENT_SENDER.lock();
    if mutex_guard.is_err() {
        return;
    }

    if let Some(sender) = &*mutex_guard.unwrap() {
        let _ = sender.send(UsbConnectionEvent::Disconnected(device_id));
    }
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_DEVICECHANGE => {
            match wparam as u32 {
                DBT_DEVICEARRIVAL => {
                    let dev_brodcast = lparam as *const DEV_BROADCAST_DEVICEINTERFACE_W;
                    if !dev_brodcast.is_null() {
                        handle_device_arrival(dev_brodcast);
                    }
                }
                DBT_DEVICEREMOVECOMPLETE => {
                    let dev_brodcast = lparam as *const DEV_BROADCAST_DEVICEINTERFACE_W;
                    if !dev_brodcast.is_null() {
                        handle_device_removal(dev_brodcast);
                    }
                }
                _ => {}
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

struct WindowHandle(HWND);

impl Deref for WindowHandle {
    type Target = HWND;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for WindowHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = DestroyWindow(self.0);
            }
        }
    }
}

struct NotificationHandle(HDEVNOTIFY);

impl Deref for NotificationHandle {
    type Target = HDEVNOTIFY;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for NotificationHandle {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                let _ = UnregisterDeviceNotification(self.0);
            }
        }
    }
}

struct WindowClass(Rc<[u16]>);

impl Deref for WindowClass {
    type Target = Rc<[u16]>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for WindowClass {
    fn drop(&mut self) {
        unsafe {
            let hinstance = GetModuleHandleW(null());
            let _ = UnregisterClassW(self.0.as_ptr(), hinstance);
        }
    }
}

pub enum UsbConnectionEvent {
    Connected(Arc<str>),
    Disconnected(Arc<str>),
}

pub struct UsbConnectionCallbacksHandle {
    event_receiver: Receiver<UsbConnectionEvent>,
    thread_finish_receiver: Receiver<Result<(), Win32Error>>,
    thread_handle: JoinHandle<Result<(), Win32Error>>,
}

impl UsbConnectionCallbacksHandle {
    pub fn setup_connection_callbacks() -> anyhow::Result<Self> {
        let (event_sender, event_receiver) = std::sync::mpsc::channel::<UsbConnectionEvent>();
        let (thread_finish_sender, thread_finish_receiver) =
            std::sync::mpsc::channel::<Result<(), Win32Error>>();

        if let Ok(mut sender_lock) = EVENT_SENDER.lock() {
            *sender_lock = Some(event_sender);
        } else {
            return Err(anyhow::anyhow!("Failed to acquire lock for EVENT_SENDER"));
        }

        let thread_handle = std::thread::spawn(move || -> Result<(), Win32Error> {
            let class_name = "UsbConnectionDetector\0"
                .encode_utf16()
                .collect::<Rc<[u16]>>();

            unsafe {
                let window_class = WNDCLASSW {
                    lpfnWndProc: Some(window_proc),
                    hInstance: GetModuleHandleW(null()),
                    lpszClassName: class_name.as_ptr(),
                    ..std::mem::zeroed()
                };
                let class_name = WindowClass(class_name.clone());

                let class_registration = RegisterClassW(&window_class as *const _);

                if class_registration == 0 {
                    if let Err(_) = thread_finish_sender.send(Err(GetLastError().into())) {
                        println!("Failed to send error from USB callback thread");
                    }
                    return Err(GetLastError().into());
                }

                let hwnd = WindowHandle(CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    class_name.as_ptr(),
                    0,
                    0,
                    0,
                    0,
                    0,
                    HWND_MESSAGE,
                    null_mut(),
                    null_mut(),
                    null_mut(),
                ));

                if hwnd.is_null() {
                    if let Err(_) = thread_finish_sender.send(Err(GetLastError().into())) {
                        println!("Failed to send error from USB callback thread");
                    }
                    return Err(GetLastError().into());
                }

                let filter = DEV_BROADCAST_DEVICEINTERFACE_W {
                    dbcc_size: std::mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as u32,
                    dbcc_devicetype: DBT_DEVTYP_DEVICEINTERFACE,
                    dbcc_classguid: GUID_DEVINTERFACE_USB_DEVICE,
                    ..std::mem::zeroed()
                };

                let notification_handle = NotificationHandle(RegisterDeviceNotificationW(
                    *hwnd,
                    &filter as *const _ as *const _,
                    DEVICE_NOTIFY_WINDOW_HANDLE,
                ));

                if notification_handle.is_null() {
                    if let Err(_) = thread_finish_sender.send(Err(GetLastError().into())) {
                        println!("Failed to send error from USB callback thread");
                    }
                    return Err(GetLastError().into());
                }

                let mut msg = std::mem::zeroed();
                loop {
                    let ret = GetMessageW(&mut msg, *hwnd, 0, 0);
                    match ret {
                        -1 => {
                            if let Err(_) = thread_finish_sender.send(Err(GetLastError().into())) {
                                println!("Failed to send error from USB callback thread");
                            }
                            return Err(GetLastError().into());
                        }
                        0 => break,
                        _ => {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                }

                Ok(())
            }
        });

        Ok(Self {
            event_receiver,
            thread_finish_receiver,
            thread_handle,
        })
    }

    pub fn poll_events(&self) -> Result<UsbConnectionEvent, PollEventError> {
        let thread_finished = self.thread_finish_receiver.try_recv();
        if thread_finished.is_ok() {
            let result = thread_finished.unwrap();
            return match result {
                Ok(_) => Err(PollEventError::ThreadFinished),
                Err(e) => Err(e.into()),
            };
        }

        self.event_receiver
            .try_recv()
            .map_err(|e| PollEventError::from(e))
    }
}
