use core::mem::ManuallyDrop;
use std::{
    future::*,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use tokio::sync::mpsc;
use windows::{
    core::*,
    Win32::Foundation::*,
    Win32::Media::Audio::*,
    Win32::Media::MediaFoundation::*,
    Win32::System::Com::StructuredStorage::*,
    Win32::System::Ole::*,
    Win32::System::Threading::*,
    //Win32::UI::WindowsAndMessaging::*,
    Win32::{Security::SECURITY_ATTRIBUTES, System::Com::*},
};

#[repr(C)]
pub struct BufferPacket {
    pub data: *const u8,
    pub size: u32,
}

#[no_mangle]
pub fn CreateCapture(
    pid: u32,
    include_process_tree: bool,
    num_channels: u16,
    num_samples_per_sec: u32,
    bits_per_sample: u16,
) -> *mut LoopbackCapture {
    let capture = LoopbackCapture::new(
        pid,
        include_process_tree,
        num_channels,
        num_samples_per_sec,
        bits_per_sample,
    );

    let capture = Box::new(capture);

    Box::into_raw(capture)
}

#[no_mangle]
pub unsafe extern "C" fn StartCaptureBlocked(
    capture: *mut LoopbackCapture,
    sample_ready_callback: unsafe extern "C" fn(*const LoopbackCapture),
) -> HRESULT {
    let mut capture = Box::from_raw(capture);

    let rt = tokio::runtime::Runtime::new().expect("Failed to instantiate tokio runtime");

    let callback = Box::new(sample_ready_callback);
    let callback = move |capture_ptr: *const LoopbackCapture| {
        callback(capture_ptr);
    };
    let callback = Box::new(callback);

    let main_task = async { capture.start(callback).await };
    let result: core::result::Result<(), HRESULT> = rt.block_on(main_task);

    Box::into_raw(capture);

    if let core::result::Result::Err(error) = result {
        error
    } else {
        HRESULT(0)
    }
}

#[no_mangle]
pub unsafe extern "C" fn GetNextPacketSize(capture: *mut LoopbackCapture) -> u32 {
    let capture = Box::from_raw(capture);

    let frames = capture
        .get_next_packet_size()
        .expect("Failed to get packet size.");

    Box::into_raw(capture);

    frames
}

#[no_mangle]
pub unsafe extern "C" fn GetBuffer(capture: *mut LoopbackCapture) -> BufferPacket {
    let mut capture = Box::from_raw(capture);

    let packet = capture.get_buffer().expect("Failed to get buffer.");

    Box::into_raw(capture);

    packet
}

#[no_mangle]
pub unsafe extern "C" fn ReleaseBuffer(capture: *mut LoopbackCapture, frames: u32) {
    let mut capture = Box::from_raw(capture);

    capture
        .release_buffer(frames)
        .expect("Failed to release buffer.");

    Box::into_raw(capture);
}

#[no_mangle]
pub unsafe extern "C" fn StopCapture(capture: *mut LoopbackCapture) {
    let mut capture = Box::from_raw(capture);

    capture.stop();

    Box::into_raw(capture);
}

#[no_mangle]
pub extern "C" fn DisposeCapture(capture: *mut LoopbackCapture) {
    unsafe { Box::from_raw(capture) };
}

pub struct LoopbackCapture {
    pid: u32,
    include_process_tree: bool,

    num_channels: u16,
    num_samples_per_sec: u32,
    bits_per_sample: u16,

    sample_ready_key: u64,
    audio_client: Option<IAudioClient>,
    capture_client: Option<IAudioCaptureClient>,
    ev_sample_ready: HANDLE,
    sample_ready_async_result: Option<IMFAsyncResult>,
    queue_id: u32,
}

impl LoopbackCapture {
    pub fn new(
        pid: u32,
        include_process_tree: bool,
        num_channels: u16,
        num_samples_per_sec: u32,
        bits_per_sample: u16,
    ) -> LoopbackCapture {
        LoopbackCapture {
            pid: pid,
            include_process_tree: include_process_tree,

            num_channels: num_channels,
            num_samples_per_sec: num_samples_per_sec,
            bits_per_sample: bits_per_sample,

            sample_ready_key: 0,
            audio_client: Option::None,
            capture_client: Option::None,
            ev_sample_ready: HANDLE(0),
            sample_ready_async_result: Option::None,
            queue_id: 0,
        }
    }

    pub async unsafe fn start(
        &mut self,
        sample_ready_callback: Box<dyn Fn(*const LoopbackCapture)>,
    ) -> core::result::Result<(), HRESULT> {

        MFStartup(MF_SDK_VERSION << 16 | MF_API_VERSION, MFSTARTUP_LITE)?;

        let mut task_id: u32 = 0;
        MFLockSharedWorkQueue("Capture", 0, &mut task_id, &mut self.queue_id)?;

        let mut audioclient_activation_params = AUDIOCLIENT_ACTIVATION_PARAMS {
            ActivationType: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            Anonymous: AUDIOCLIENT_ACTIVATION_PARAMS_0 {
                ProcessLoopbackParams: AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS {
                    TargetProcessId: self.pid,
                    ProcessLoopbackMode: if self.include_process_tree {
                        PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE
                    } else {
                        PROCESS_LOOPBACK_MODE_EXCLUDE_TARGET_PROCESS_TREE
                    },
                },
            },
        };

        let acrivate_params_var = ManuallyDrop::new(PROPVARIANT_0_0 {
            vt: VT_BLOB.0 as u16,
            Anonymous: PROPVARIANT_0_0_0 {
                blob: BLOB {
                    cbSize: std::mem::size_of::<AUDIOCLIENT_ACTIVATION_PARAMS>() as u32,
                    pBlobData: (&mut audioclient_activation_params)
                        as *mut AUDIOCLIENT_ACTIVATION_PARAMS
                        as *mut u8,
                },
            },
            ..Default::default()
        });

        let acrivate_params = PROPVARIANT {
            Anonymous: PROPVARIANT_0 {
                Anonymous: acrivate_params_var,
            },
        };

        let completion_handler = CompletionHandler::new();
        let completion_handler: IActivateAudioInterfaceCompletionHandler =
            completion_handler.into();

        let op = ActivateAudioInterfaceAsync(
            VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK,
            &IAudioClient::IID as *const GUID,
            &acrivate_params as *const PROPVARIANT,
            &completion_handler,
        )?;

        let completion_handler = CompletionHandler::to_impl(&completion_handler);

        completion_handler.await;

        let mut activate_result = HRESULT(0);
        let mut activated_interface: Option<IUnknown> = Option::None;

        op.GetActivateResult(
            &mut activate_result as *mut HRESULT,
            &mut activated_interface as *mut Option<IUnknown>,
        )?;
        activate_result.ok()?;

        let activated_interface = activated_interface.ok_or(E_FAIL)?;
        let audio_client: IAudioClient = core::mem::transmute(activated_interface);
        self.audio_client = Option::Some(audio_client);
        let audio_client = self.audio_client.as_ref().unwrap();

        let num_block_align: u16 = self.num_channels * self.bits_per_sample / 8u16;
        let num_avg_bytes_per_sec: u32 = self.num_samples_per_sec * num_block_align as u32;

        let capture_format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: self.num_channels,
            nSamplesPerSec: self.num_samples_per_sec,
            wBitsPerSample: self.bits_per_sample,
            nBlockAlign: num_block_align,
            nAvgBytesPerSec: num_avg_bytes_per_sec,
            ..Default::default()
        };

        audio_client.Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK,
            200000,
            AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM as i64,
            &capture_format as *const WAVEFORMATEX,
            &GUID::zeroed(),
        )?;

        let mut capture_client: Option<IAudioCaptureClient> = Option::None;
        audio_client.GetService(
            &IAudioCaptureClient::IID,
            core::mem::transmute(&mut capture_client as *mut Option<IAudioCaptureClient>),
        )?;

        let capture_client = capture_client.ok_or(E_FAIL)?;
        self.capture_client = Option::Some(capture_client);

        fn brid(loopback_ptr: *const LoopbackCapture) {}

        let sample_capturer: IMFAsyncCallback = AsyncCallback::new_callback(
            self.queue_id,
            32,
            Option::Some(sample_ready_callback),
            self as *const LoopbackCapture,
        )
        .into();

        let ev_sample_ready = CreateEventW(
            std::ptr::null() as *const SECURITY_ATTRIBUTES,
            false,
            false,
            Option::None,
        )?;

        let async_result = MFCreateAsyncResult(Option::None, &sample_capturer, Option::None)?;
        self.sample_ready_async_result = Option::Some(async_result);

        audio_client.SetEventHandle(ev_sample_ready)?;

        let start_capture: IMFAsyncCallback =
            AsyncCallback::new(MFASYNC_CALLBACK_QUEUE_MULTITHREADED, 32).into();
        MFPutWorkItem2(
            MFASYNC_CALLBACK_QUEUE_MULTITHREADED,
            0,
            &start_capture,
            Option::None,
        )?;

        let start_capture = AsyncCallback::to_impl(&start_capture);
        start_capture.recv().await;

        audio_client.Start()?;

        self.sample_ready_key =
            MFPutWaitingWorkItem(ev_sample_ready, 0, &self.sample_ready_async_result)?;

        self.ev_sample_ready = ev_sample_ready;

        core::result::Result::Ok(())

    }

    pub unsafe fn get_next_packet_size(&self) -> windows::core::Result<u32> {
        self.capture_client.as_ref().unwrap().GetNextPacketSize()
    }

    pub unsafe fn get_buffer(&mut self) -> windows::core::Result<BufferPacket> {
        let mut data_ptr = 0 as *mut u8;

        let mut frames: u32 = 0;
        let mut dw_capture_flags: u32 = 0;
        let mut device_position: u64 = 0;
        let mut qpc_position: u64 = 0;

        self.capture_client.as_ref().unwrap().GetBuffer(
            &mut data_ptr as *mut *mut u8,
            &mut frames as *mut u32,
            &mut dw_capture_flags as *mut u32,
            &mut device_position as *mut u64,
            &mut qpc_position as *mut u64,
        )?;

        let num_block_align: u16 = self.num_channels * self.bits_per_sample / 8u16;


        Result::Ok(BufferPacket {
            data: data_ptr,
            size: frames * num_block_align as u32,
        })
    }

    pub unsafe fn release_buffer(&mut self, frames: u32) -> windows::core::Result<()> {
        self.capture_client
            .as_ref()
            .unwrap()
            .ReleaseBuffer(frames)?;

        self.sample_ready_key =
            MFPutWaitingWorkItem(self.ev_sample_ready, 0, &self.sample_ready_async_result)?;

        Result::Ok(())
    }

    pub unsafe fn stop(&mut self) {
        if self.sample_ready_key != 0 {
            MFCancelWorkItem(self.sample_ready_key);
            self.sample_ready_key = 0;
        }

        if let Option::Some(client) = &self.audio_client {
            client.Stop();
            self.audio_client = Option::None;
        }

        self.sample_ready_async_result = Option::None;

        if self.queue_id != 0 {
            MFUnlockWorkQueue(self.queue_id);
            self.queue_id = 0;
        }
    }
}

#[implement(IMFAsyncCallback)]
struct AsyncCallback {
    queue_id: u32,
    receiver: Option<mpsc::Receiver<()>>,
    sender: Option<mpsc::Sender<()>>,
    callback: Option<Box<dyn Fn(*const LoopbackCapture)>>,
    capture_ptr: *const LoopbackCapture,
}

impl AsyncCallback {
    fn new_callback(
        queue_id: u32,
        buffer: usize,
        callback: Option<Box<dyn Fn(*const LoopbackCapture)>>,
        capture_ptr: *const LoopbackCapture,
    ) -> AsyncCallback {
        AsyncCallback {
            queue_id: queue_id,
            sender: None,
            receiver: None,
            callback: callback,
            capture_ptr: capture_ptr,
        }
    }

    fn new(queue_id: u32, buffer: usize) -> AsyncCallback {
        let (tx, rx) = mpsc::channel(buffer);
        AsyncCallback {
            queue_id: queue_id,
            sender: Some(tx),
            receiver: Some(rx),
            callback: None,
            capture_ptr: 0 as *const LoopbackCapture,
        }
    }

    async fn recv(&mut self) -> Option<()> {
        self.receiver.as_mut().unwrap().recv().await
    }
}

impl IMFAsyncCallback_Impl for AsyncCallback {
    fn GetParameters(&self, pdwflags: *mut u32, pdwqueue: *mut u32) -> windows::core::Result<()> {
        unsafe {
            *pdwflags = 0;
            *pdwqueue = self.queue_id;
        }
        Result::Ok(())
    }

    fn Invoke(&self, result: &core::option::Option<IMFAsyncResult>) -> windows::core::Result<()> {
        if let Some(sender) = self.sender.as_ref() {
            sender.blocking_send(()).expect("blocking_send() failed.");
        }
        
        if let Option::Some(c) = self.callback.as_ref() {
            c(self.capture_ptr);
        }
        Result::Ok(())
    }
}

#[implement(IActivateAudioInterfaceCompletionHandler)]
struct CompletionHandler {
    completed: bool,
    waker: Option<Waker>,
}

impl CompletionHandler {
    fn new() -> CompletionHandler {
        CompletionHandler {
            completed: false,
            waker: Option::None,
        }
    }
}

impl IActivateAudioInterfaceCompletionHandler_Impl for CompletionHandler {
    fn ActivateCompleted(&self, _: &Option<IActivateAudioInterfaceAsyncOperation>) -> Result<()> {
        let self_ptr = self as *const CompletionHandler as *mut CompletionHandler;
        unsafe {
            (*self_ptr).completed = true;
        }

        if let Option::Some(waker) = &self.waker {
            waker.clone().wake();
        };

        return Result::Ok(());
    }
}

impl Future for CompletionHandler {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.completed {
            Poll::Ready(())
        } else {
            self.waker = Option::Some(cx.waker().clone());
            Poll::Pending
        }
    }
}
