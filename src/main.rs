extern crate winapi;
extern crate backtrace;

mod noise;
mod envelope;

use noise::noise::{NoiseMaker, NoiseArgs};
use envelope::envelope::EnvelopeADSR;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicPtr, Ordering};
use std::sync::{Mutex, Arc, Condvar, Weak};
use std::f64::consts::PI;
use std::mem;
use std::thread;
use std::ptr;
use winapi::um::winuser;
use num::pow;
use backtrace::Backtrace;
//use crate::noise::noise::NoiseMaker;


fn hz_to_angular(hertz: f64) -> f64 {
    return hertz * 2.0 * PI;
}

fn oscillate(hertz: f64, current_time: f64, osc_type: i32) -> f64 {
    return match osc_type {
        0 => f64::sin(hz_to_angular(hertz) * current_time),
        1 => if f64::sin(hz_to_angular(hertz) * current_time) > 0.0 { 1.0 } else { -1.0 },
        2 => f64::asin(f64::sin(hz_to_angular(hertz) * current_time)) * (2.0 / PI),
        3 => {
            let mut output = 0.0;
            for n in 1 .. 100 {
                output += (f64::sin(n as f64 * hz_to_angular(hertz) * current_time)) / n as f64;
            }
            return output * (2.0 / PI);
        },
        4 => (2.0 / PI) * (hertz * PI * (current_time % (1.0 / hertz)) - (PI / 2.0)),
        _ => 0.0
    }
}

fn make_noise(current_time: f64) -> f64 {
    //let output: f64 = 
    //println!("Making noise!!");
    return 0.5 * f64::sin(110.0 * 2 as f64 * PI * current_time);
    //return 0.1;
}

//static mut FREQUENCY_OUTPUT: f64 = 0.0;
static mut OCTAVE_BASE_FREQUENCY: f64 = 110.0; // A2
static mut CURRENT_KEY: i32 = -1;
static mut KEY_PRESSED: bool = false;
static mut TWELVE_ROOT_OF_TWO: f64 = 0.0;

fn main() {
    let bt = Backtrace::new();
    println!("Hello, world!");

    //let sound = NoiseMaker::create();
    //NoiseMaker::new()
    unsafe {
        let devices: Vec<String> = NoiseMaker::enumerate();
    
        for d in devices.iter() {
            println!("Found Output Device {}\n", d);
        }
    
        //let mut temporal = Box::new(100);
        let mut sound: NoiseMaker = NoiseMaker::new(NoiseArgs::default());
        sound.create((*devices[0]).to_string());
        sound.set_user_function(make_noise);

        //TWELVE_ROOT_OF_TWO = num::pow(2.0, 1.0 / 12.0);
        TWELVE_ROOT_OF_TWO = 1.0594630943592952645618252949463;
        println!("TWELVE_ROOT_OF_TWO = {}", TWELVE_ROOT_OF_TWO);
        //FREQUENCY_OUTPUT = Box::new(0.0);

        // iflet mut current_key: i32 = -1;
        //let mut frequency_output: f64 = Box::new()
        let mut envelope: EnvelopeADSR = EnvelopeADSR::new();
        //let mut key_pressed: bool = false;
        
        //let key_pressed = AtomicBool::new(false);


        // let new_thread = thread::spawn(|| {
        //     loop {

        //     }
        // });

        // loop {
        //     let a = 10;
        //     println!("a = {}", a);
        // }

        

        loop {
            KEY_PRESSED = false;
            for k in 0 .. 16 {
                // White and black keys
                if winuser::GetAsyncKeyState(b"ZSXCFVGBNJMK\xbcL\xbe\xbf"[k] as i32) as u16 & 0x8000 != 0 {
                    if CURRENT_KEY != k as i32 {
                        sound.set_frequency_output(OCTAVE_BASE_FREQUENCY * num::pow(TWELVE_ROOT_OF_TWO, k));
                        sound.set_envelop_note_on();
                        println!("\rNote On : {}s {}Hz", sound.get_time(), sound.get_frequency_output());
                        CURRENT_KEY = k as i32;
                    }
                    KEY_PRESSED = true;
                }
            }

            if !KEY_PRESSED {
                if CURRENT_KEY != -1 {
                    println!("\rNote Off : {}s", sound.get_time());
                    sound.set_frequency_output(0.0);
                    sound.set_envelope_note_off();
                    CURRENT_KEY = -1;
                }
            }
        }

        println!("Out of loop");
    }
}
