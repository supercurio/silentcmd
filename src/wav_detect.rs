#[macro_use]
extern crate serde_derive;
extern crate docopt;
extern crate hound;
extern crate sample;

pub mod common;

use docopt::Docopt;

use sample::{signal, Signal};
use sample::envelope;
use sample::ring_buffer;
use sample::{I24, Sample};

use std::process;
use std::i16;
use std::i32;


const USAGE: &str = "
Silent Command for WAV file.

Usage:
  silentcmd-wav <file> [--window=<samples>]
  silentcmd --version

Options:
  -h --help             Show this screen.
  <file>                WAV input file.
  --window=<samples>    Window size in samples [default: 1024].
";

#[derive(Debug, Deserialize)]
struct Args {
    arg_file: String,
    flag_window: usize,
}

fn main() {
    let args: Args = Docopt::new(USAGE)
        .and_then(|d| d.deserialize())
        .unwrap_or_else(|e| e.exit());

    eprintln!("Detecting signal from file: {}", args.arg_file);
    eprintln!("Window size: {} samples", args.flag_window);

    let mut reader = hound::WavReader::open(args.arg_file).unwrap();
    eprintln!("Spec: {:?}", reader.spec());

    if reader.spec().channels != 1 {
        eprintln!("Input file must be mono (1 channel instead of {}).", reader.spec().channels);
        process::exit(1);
    }
    let bit_per_sample = reader.spec().bits_per_sample;

    let ring_buffer = ring_buffer::Fixed::from(vec![[0.0]; args.flag_window]);

    let mut total = Vec::new();
    loop {
        let buf = match reader.spec().sample_format {
            hound::SampleFormat::Int => {
                reader.samples::<i32>()
                    .take(args.flag_window)
                    .filter_map(Result::ok)
                    .map(|s|
                        match bit_per_sample {
                            16 => s as f32 / f32::from(i16::MAX),
                            24 => I24::new(s).unwrap().to_float_sample(),
                            32 => s as f32 / i32::MAX as f32,
                            _ => 0.0,
                        })
                    .collect::<Vec<_>>()
            }
            hound::SampleFormat::Float => {
                reader.samples::<f32>()
                    .take(args.flag_window)
                    .filter_map(Result::ok)
                    .collect::<Vec<_>>()
            }
        };

        if buf.is_empty() {
            break;
        }
        let frame = signal::from_interleaved_samples_iter::<_, [f32; 1]>(buf.iter().cloned());

        let attack = 1.0;
        let release = 1.0;

        let detector = envelope::Detector::rms(ring_buffer.clone(), attack, release);
        let envelope = frame.detect_envelope(detector);

        let last = envelope.until_exhausted().last().unwrap()[0];
        println!("{:?}", common::to_db(last));
        total.push(last);
    }

    let avg = total.iter().sum::<f32>() / total.len() as f32;

    eprintln!("Average: {}, {} dB", avg, common::to_db(avg));
}
