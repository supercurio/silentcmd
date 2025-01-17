use std::time::{Duration, Instant};
use std::process::Command;
use std::thread;
use std::sync::mpsc;

pub struct SwitchStatus {
    threshold_db: f32,
    timeout_s: Duration,
    on_trigger_last: Instant,
    is_on: bool,
    tx: mpsc::Sender<bool>,
}

impl SwitchStatus {
    pub fn new(threshold_db: f32, timeout_s: u64, tx: mpsc::Sender<bool>) -> SwitchStatus {
        SwitchStatus {
            threshold_db,
            timeout_s: Duration::from_secs(timeout_s),
            on_trigger_last: Instant::now(),
            is_on: false,
            tx,
        }
    }

    pub fn start(cmd_on: String, cmd_off: String, rx: mpsc::Receiver<bool>) {
        thread::spawn(move || {
            for state in rx {
                let cmd = if state { cmd_on.clone() } else { cmd_off.clone() };
                println!("Run {:?}", cmd);
                Command::new(cmd)
                    .spawn()
                    .expect("Unable to run command")
                    .wait()
                    .unwrap();
            }
        });
    }

    pub fn update_level(&mut self, level: f32) {
        if level >= self.threshold_db {
            self.on_trigger_last = Instant::now();
            if !self.is_on {
                self.turn_on();
            }
        } else if self.is_on &&
            Instant::now().duration_since(self.on_trigger_last) > self.timeout_s {
            self.turn_off();
        }
    }

    pub fn is_on(&self) -> bool {
        self.is_on
    }

    fn turn_on(&mut self) {
        eprintln!("Turn on");
        self.tx.send(true).unwrap();
        self.is_on = true;
    }

    fn turn_off(&mut self) {
        eprintln!("Turn off");
        self.tx.send(false).unwrap();
        self.is_on = false;
    }
}
