// Wait for any touch/key event on Kindle's input devices.
// Used to dismiss the weather screen back to the Kindle UI.

use std::fs::File;
use std::io::Read;
use std::sync::mpsc;
use std::thread;

// armv7 (32-bit) struct input_event layout:
//   0..8   timeval (sec u32 + usec u32)
//   8..10  type (u16)
//   10..12 code (u16)
//   12..16 value (i32)
const EVENT_SIZE: usize = 16;
const EV_SYN: u16 = 0;
const EV_MSC: u16 = 4;

pub fn wait_for_input() {
    let (tx, rx) = mpsc::channel::<()>();
    for i in 0..4 {
        let path = format!("/dev/input/event{}", i);
        let tx = tx.clone();
        thread::spawn(move || {
            let mut f = match File::open(&path) {
                Ok(f) => f,
                Err(_) => return,
            };
            let mut buf = [0u8; EVENT_SIZE];
            while f.read_exact(&mut buf).is_ok() {
                let etype = u16::from_le_bytes([buf[8], buf[9]]);
                // Anything that isn't a sync/misc filler counts as user input
                if etype != EV_SYN && etype != EV_MSC {
                    let _ = tx.send(());
                    return;
                }
            }
        });
    }
    drop(tx);
    let _ = rx.recv();
}
