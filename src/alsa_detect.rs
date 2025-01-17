#[macro_use]
extern crate serde_derive;
extern crate docopt;
extern crate alsa;
extern crate sample;

pub mod common;
pub mod switch;

use std::collections::HashSet;
use docopt::Docopt;
use alsa::{Direction, ValueOr};
use alsa::pcm::{PCM, HwParams, Format, Access};
use sample::{signal, Signal, envelope, ring_buffer, Sample};
use std::sync::mpsc;
use switch::SwitchStatus;

const USAGE: &str = "
Silent Command for ALSA.

Usage:
  silentcmd-alsa <cmd-on> <cmd-off> [--device=<alsa-device> --channels=<1,2> --threshold=<db> --timeout=<s> --sample-rate=<Hz> --buffer-size=<samples> --bits=<resolution> --verbose]

Options:
  -h --help                 Show this screen.
  --device=<alsa-device>    ALSA device to record from [default: default]
  --channels=<1,2,4>        List of channel numbers to record from [default: 1]
  --threshold=<db>          Minimal signal level to turn on [default: -60.0]
  --timeout=<s>             Amount of time without signal before off switch [default: 30]
  --bits=<value>            ALSA device to record from: 16/24/32 [default: 32]
  --buffer-size=<samples>   Buffer and window size in samples [default: 1024].
  --sample-rate=<Hz>        Recording sample rate [default: 48000].
  --verbose                 Print level and status on stdout.
";

#[derive(Debug, Deserialize)]
struct Args {
    arg_cmd_on: String,
    arg_cmd_off: String,
    flag_device: String,
    flag_buffer_size: usize,
    flag_channels: String,
    flag_threshold: f32,
    flag_timeout: u64,
    flag_bits: u32,
    flag_sample_rate: u32,
    flag_verbose: bool,
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    // validate channels
    let channels: HashSet<usize> = args.flag_channels
        .trim()
        .split(',')
        .map(|s| s.parse().unwrap())
        .collect();
    let channel_count = *channels.iter().max().unwrap();

    let alsa_device_name = args.flag_device;
    eprintln!("Recording {} channels from ALSA device: {}, keeping channel(s) {:?}",
              channel_count,
              alsa_device_name,
              channels);

    let pcm = PCM::new(&alsa_device_name, Direction::Capture, false).unwrap();

    let hwp = HwParams::any(&pcm).unwrap();
    hwp.set_channels(channel_count as u32).unwrap();
    hwp.set_rate(args.flag_sample_rate, ValueOr::Nearest).unwrap();
    hwp.set_format(
        match args.flag_bits {
            16 => Format::s16(),
            24 => Format::s24(),
            _ => Format::s32(),
        }).unwrap();
    hwp.set_access(Access::RWInterleaved).unwrap();
    pcm.hw_params(&hwp).unwrap();

    let hwp = pcm.hw_params_current().unwrap();
    eprintln!("HW buffer size: {}, period size: {}, periods: {}",
              hwp.get_buffer_size().unwrap(),
              hwp.get_period_size().unwrap(),
              hwp.get_periods().unwrap());

    let buf_size = args.flag_buffer_size;
    let ring_buffer = ring_buffer::Fixed::from(vec![[0.0]; buf_size]);
    let (tx, rx) = mpsc::channel();
    let mut switch = SwitchStatus::new(args.flag_threshold,
                                       args.flag_timeout,
                                       tx);
    SwitchStatus::start(args.arg_cmd_on, args.arg_cmd_off, rx);

    match args.flag_bits {
        16 => {
            let mut rec_buf_i16 = vec![0; buf_size * channel_count];
            let mut de_interleaved_i32 = vec![0; buf_size];

            loop {
                let io = pcm.io_i16().unwrap();
                let read = io.readi(rec_buf_i16.as_mut_slice());
                match read {
                    Ok(size) => eprintln!("read {} frames", size),
                    Err(e) => eprintln!("Error: {}", e),
                };

                // de-interleave
                for i in 0..buf_size {
                    let mut val: i32 = 0;
                    for c in 0..channel_count {
                        if channels.contains(&(c + 1)) {
                            val += i32::from(rec_buf_i16[i * channel_count + c])
                        }
                    }
                    de_interleaved_i32[i] = (val / channel_count as i32)
                        .to_sample::<i16>()
                        .to_sample::<i32>();
                }

                process_buf(&de_interleaved_i32,
                            ring_buffer.clone(),
                            &mut switch,
                            args.flag_verbose);
            }
        }
        _ => {
            let mut rec_buf_i32 = vec![0i32; buf_size * channel_count];
            let mut de_interleaved_i32 = vec![0i32; buf_size];
            loop {
                let io = pcm.io_i32().unwrap();
                io.readi(rec_buf_i32.as_mut_slice()).unwrap();

                // de-interleave
                for i in 0..buf_size {
                    let mut val: i64 = 0;
                    for c in 0..channel_count {
                        val += i64::from(rec_buf_i32[i * channel_count + c])
                    }
                    de_interleaved_i32[i] = (val / channel_count as i64) as i32;
                }

                process_buf(&de_interleaved_i32,
                            ring_buffer.clone(),
                            &mut switch,
                            args.flag_verbose);
            }
        }
    }
}

fn process_buf(rec_buf: &[i32],
               ring_buffer: ring_buffer::Fixed<Vec<[f32; 1]>>,
               switch: &mut SwitchStatus,
               print: bool) {
    let frame = signal::from_interleaved_samples_iter::<_, [i32; 1]>(rec_buf.iter().cloned());

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
