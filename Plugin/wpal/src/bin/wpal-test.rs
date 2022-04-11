extern crate wpal;

use windows::core::HRESULT;

use wpal::*;

#[tokio::main]
async fn main() {
    println!("PID? ");
    let mut pid_str = String::new();
    let mut pid: u32 = 0;
    loop {
        std::io::stdin()
            .read_line(&mut pid_str)
            .expect("Failed to read stdin.");

        pid = match pid_str.trim().parse() {
            Ok(num) => num,
            Err(_) => continue,
        };
        break;
    }

    let mut capture = LoopbackCapture::new(pid, true, 2, 44100, 16);

    let callback = |capture_ptr: *const LoopbackCapture| unsafe {
        let capture_ptr = capture_ptr as *mut LoopbackCapture;
        let frames = (*capture_ptr)
            .get_next_packet_size()
            .expect("Failed to get next packet size");

        if frames <= 0 {
            return;
        }

        let packet = (*capture_ptr).get_buffer().expect("Failed to get buffer");

        println!(
            "{} frames, {} bytes, first: {}",
            frames,
            packet.size,
            (*packet.data)
        );

        (*capture_ptr)
            .release_buffer(frames)
            .expect("Failed to release buffer");
    };
    let callback = Box::new(callback);

    unsafe {
        capture.start(callback).await.expect("Failed to start.");
    };

    loop {
        tokio::time::sleep(std::time::Duration::from_millis(10000)).await;
    }

    println!("Finished");
}
