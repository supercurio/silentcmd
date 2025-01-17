#[macro_use]
extern crate serde_derive;
extern crate docopt;
extern crate jack;
extern crate sample;

pub mod common;
pub mod switch;

use std::io;
use docopt::Docopt;
use sample::{signal, Signal, envelope, ring_buffer};
use std::sync::mpsc;
use switch::SwitchStatus;

const USAGE: &str = "
Silent Command JACK plugin.

Usage:
  silentcmd-jack <cmd-on> <cmd-off> [--threshold=<db> --timeout=<s> --verbose]

Options:
  -h --help         Show this screen.
  --threshold=<db>  Minimal signal level to turn on [default: -40.0]
  --timeout=<s>     Amount of time without signal before off switch [default: 60]
  --verbose         Print level and status on stdout.
";

#[derive(Debug, Deserialize)]
struct Args {
    arg_cmd_on: String,
    arg_cmd_off: String,
    flag_threshold: f32,
    flag_timeout: u64,
    flag_verbose: bool,
}

fn main() {
    // process command line arguments
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    // Create client
    let (client, _status) =
        jack::Client::new("silentcmd", jack::ClientOptions::NO_START_SERVER).unwrap();

    // Register ports. They will be used in a callback that will be
    // called when new data is available.
    let in_port = client.register_port("in_1", jack::AudioIn::default()).unwrap();

    let buffer_size = client.buffer_size() as usize;
    let verbose = args.flag_verbose;

    let ring_buffer = ring_buffer::Fixed::from(vec![[0.0]; buffer_size]);
    let (tx, rx) = mpsc::channel();
    let mut switch = SwitchStatus::new(args.flag_threshold,
                                       args.flag_timeout,
                                       tx);
    SwitchStatus::start(args.arg_cmd_on, args.arg_cmd_off, rx);

    let process_callback = move |_: &jack::Client, ps: &jack::ProcessScope| -> jack::Control {
        let in_port_p = in_port.as_slice(ps);

        // process the buffer
        process_buf(in_port_p, ring_buffer.clone(), &mut switch, verbose);

        // Continue as normal
        jack::Control::Continue
    };

    let process = jack::ClosureProcessHandler::new(process_callback);

    // Activate the client, which starts the processing.
    let active_client = client.activate_async(Notifications, process).unwrap();

    // Wait for user input to quit
    // TODO: find a better method to keep the plugin alive
    println!("Press enter/return to quit...");
    let mut user_input = String::new();
    io::stdin().read_line(&mut user_input).ok();

    active_client.deactivate().unwrap();
}

fn process_buf(rec_buf: &[f32],
               ring_buffer: ring_buffer::Fixed<Vec<[f32; 1]>>,
               switch: &mut SwitchStatus,
               print: bool) {
    let frame = signal::from_interleaved_samples_iter::<_, [f32; 1]>(rec_buf.iter().cloned());

    let detector = envelope::Detector::rms(ring_buffer,
                                           common::ATTACK,
                                           common::RELEASE);
    let envelope = frame.detect_envelope(detector);

    let last = envelope.until_exhausted().last().unwrap()[0];

    let db = common::to_db(last);
    switch.update_level(db);

    if print {
        println!("{:?}\t{:?}", common::to_db(last), if switch.is_on() { 20.0 } else { 0.0 });
    }
}

struct Notifications;

impl jack::NotificationHandler for Notifications {
    fn thread_init(&self, _: &jack::Client) {
        println!("JACK: thread init");
    }

    fn shutdown(&mut self, status: jack::ClientStatus, reason: &str) {
        println!(
            "JACK: shutdown with status {:?} because \"{}\"",
            status, reason
        );
    }

    fn freewheel(&mut self, _: &jack::Client, is_enabled: bool) {
        println!(
            "JACK: freewheel mode is {}",
            if is_enabled { "on" } else { "off" }
        );
    }

    fn buffer_size(&mut self, _: &jack::Client, sz: jack::Frames) -> jack::Control {
        println!("JACK: buffer size changed to {}", sz);
        jack::Control::Continue
    }

    fn sample_rate(&mut self, _: &jack::Client, srate: jack::Frames) -> jack::Control {
        println!("JACK: sample rate changed to {}", srate);
        jack::Control::Continue
    }

    fn client_registration(&mut self, _: &jack::Client, name: &str, is_reg: bool) {
        println!(
            "JACK: {} client with name \"{}\"",
            if is_reg { "registered" } else { "unregistered" },
            name
        );
    }

    fn port_registration(&mut self, _: &jack::Client, port_id: jack::PortId, is_reg: bool) {
        println!(
            "JACK: {} port with id {}",
            if is_reg { "registered" } else { "unregistered" },
            port_id
        );
    }

    fn port_rename(
        &mut self,
        _: &jack::Client,
        port_id: jack::PortId,
        old_name: &str,
        new_name: &str,
    ) -> jack::Control {
        println!(
            "JACK: port with id {} renamed from {} to {}",
            port_id, old_name, new_name
        );
        jack::Control::Continue
    }

    fn ports_connected(
        &mut self,
        _: &jack::Client,
        port_id_a: jack::PortId,
        port_id_b: jack::PortId,
        are_connected: bool,
    ) {
        println!(
            "JACK: ports with id {} and {} are {}",
            port_id_a,
            port_id_b,
            if are_connected {
                "connected"
            } else {
                "disconnected"
            }
        );
    }

    fn graph_reorder(&mut self, _: &jack::Client) -> jack::Control {
        println!("JACK: graph reordered");
        jack::Control::Continue
    }

    fn xrun(&mut self, _: &jack::Client) -> jack::Control {
        println!("JACK: xrun occurred");
        jack::Control::Continue
    }

    fn latency(&mut self, _: &jack::Client, mode: jack::LatencyType) {
        println!(
            "JACK: {} latency has changed",
            match mode {
                jack::LatencyType::Capture => "capture",
                jack::LatencyType::Playback => "playback",
            }
        );
    }
}
