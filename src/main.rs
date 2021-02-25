
mod noise;
use noise::noise::{NoiseMaker, NoiseArgs};
use std::f64::consts::PI;
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
    return 0.5 * f64::sin(440.0 * 2 as f64 * PI * current_time);
    //return 0.1;
}

fn main() {
    println!("Hello, world!");

    //let sound = NoiseMaker::create();
    //NoiseMaker::new()
    unsafe {
        let devices: Vec<String> = NoiseMaker::enumerate();
    
        for d in devices.iter() {
            println!("Found Output Device {}\n", d);
        }
    
        let mut sound: NoiseMaker = NoiseMaker::new(NoiseArgs::default());
        sound.set_user_function(make_noise);
    }
}
