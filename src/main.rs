
mod noise;
use noise::noise::{NoiseMaker, NoiseArgs};
//use crate::noise::noise::NoiseMaker;

fn main() {
    println!("Hello, world!");

    //let sound = NoiseMaker::create();
    //NoiseMaker::new()

    let sound: NoiseMaker<u16> = NoiseMaker::new(NoiseArgs::default());
    
}
