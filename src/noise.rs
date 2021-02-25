
pub mod noise {

    extern crate winapi;
    extern crate num;
    
    use num::pow;
    use num::Integer;

    use std::thread;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::vec::Vec;
    use std::ptr;
    use std::mem;

    use winapi::um::mmsystem;
    use winapi::um::mmeapi;
    use winapi::shared::mmreg;
    use winapi::shared::minwindef;

    pub struct NoiseArgs {
        pub sample_rate: u32,
        pub channels: u32,
        pub blocks: u32,
        pub block_samples: u32
    }

    impl Default for NoiseArgs {
        fn default() -> Self { 
            Self { sample_rate: 44100, channels: 1, blocks: 8, block_samples: 512 } 
        }
    }

    trait Noise {
        fn user_process(&self, delta_time: f64) -> f64;
    }

    pub struct NoiseMaker<T: Integer> {
        user_function: *const fn(f64) -> f64,
        sample_rate: u32,
        channels: u32,
        block_count: u32,
        block_samples: u32,
        block_current: u32,
        block_memory: *mut T,
        wave_headers: Vec<mmsystem::WAVEHDR>, // Array of headers
        hw_device: mmsystem::HWAVEOUT, // Chosen device
        ready: AtomicBool,
        block_free: AtomicU32,
        thread: thread::JoinHandle<()>,
        global_time: f64
    }
    
    impl<T: 'static + Integer> NoiseMaker<T> {
        pub fn new(args: NoiseArgs) -> Self {
            let mut obj = Self { 
                user_function: ptr::null(),
                sample_rate: args.sample_rate,
                channels: args.channels,
                block_count: args.blocks,
                block_samples: args.block_samples,
                block_current: 0,
                block_memory: ptr::null_mut(),
                block_free: AtomicU32::new(args.blocks),
                wave_headers: Vec::<mmsystem::WAVEHDR>::new(),
                ready: AtomicBool::new(false),
                thread: thread::spawn(|| {}),
                global_time: 0.0,
                hw_device: ptr::null_mut(),
            };

            return obj;
        }

        fn create(&mut self, output_device: String) -> bool {
            //self.thread = thread::spawn(self.main_thread);
            let mut devices: Vec<String> = Vec::new();
            let mut devices_iter = devices.iter();

            let find_res = devices_iter.position(|&x| x == output_device);
            if devices_iter.count() != 0 {
                // Device is available

                let device_id: usize = match find_res {
                    Some(x) => x,
                    None => 0
                };

                let wave_format: mmreg::WAVEFORMATEX;
                wave_format.wFormatTag = mmreg::WAVE_FORMAT_PCM;
                wave_format.nSamplesPerSec = self.sample_rate;
                wave_format.wBitsPerSample = (mem::size_of::<T>() * 8) as u16;
                wave_format.nChannels = self.channels as u16;
                wave_format.nBlockAlign = (wave_format.wBitsPerSample / 8) * wave_format.nChannels;
                wave_format.nAvgBytesPerSec = wave_format.nSamplesPerSec * wave_format.nBlockAlign as u32;
                wave_format.cbSize = 0;
            }

            // Open Device if valid
            if waveOutOpen(&self.hw_device, device_id, &wave_format, )

            return true;
        }

        pub fn main_thread(&mut self) {
            self.global_time = 0.0;
            let time_step: f64 = 1.0 / self.sample_rate as f64;

            let max_sample: i32 = pow(2, (std::mem::size_of::<T>() * 8) - 1) - 1;

            let dmax_sample: f64 = max_sample as f64;
            let previous_sample = 0;

            while self.ready.load(Ordering::Relaxed) {

                // Wait for block to become available
                if self.block_free.load(Ordering::Relaxed) == 0 {
                    
                }

                // Block is here, so use it
                *self.block_free.get_mut() -= 1;

                // Prepare block for processing
                if self.wave_headers[self.block_current as usize].dw_flags & mmsystem::WHDR_PREPARED {
                    mmeapi::waveOutUnprepareHeader(self.hw_device, &mut self.wave_headers[self.block_current as usize], mem::size_of::<mmsystem::WAVEHDR>() as u32);
                }

                let new_sample = 0;
                let current_block: u32 = self.block_current * self.block_samples;

                for n in 0 .. self.block_samples {
                    if self.user_function == ptr::null_mut() {
                        new_sample = (clip(user_process(self.global_time), 1.0) * dmax_sample) as T;
                    } else {
                        new_sample = (clip(self.user_function(self.global_time), 1.0) * dmax_sample) as T;
                    }

                    self.block_memory[current_block + n] = new_sample;
                    previous_sample = new_sample;

                    self.global_time += time_step;
                }
            }
        }

        fn wave_out_proc(self, wave_out: mmsystem::HWAVEOUT, msg: u32, dw_param1: minwindef::DWORD, dw_param2: minwindef::DWORD) {
            if msg != mmsystem::WOM_DONE {
                return;
            }

            *self.block_free.get_mut() += 1;
            
        }

        fn wave_out_proc_wrapper(wave_out: mmsystem::HWAVEOUT, msg: u32, dw_instance: minwindef::DWORD, dw_param1: minwindef::DWORD, dw_param2: minwindef::DWORD) {
            (dw_instance as *mut NoiseMaker::<T>).wave_out_proc(wave_out, msg, dw_param1, dw_param2);
        }

        pub fn enumerate() -> Vec<String> {
            //let deviceCount: u32 = waveOutGetNumDevs();
            let devices: Vec<String> = Vec::new();
            
            return devices;
        }

        pub fn set_user_function(&mut self, func: *mut fn(f64) -> f64) {
            self.user_function = func;
        }

        pub fn clip(sample: f64, max: f64) -> f64 {
            return if sample >= 0.0 { min_clip(sample, max) } else { max_clip(sample, -max) };
        }
    }

    fn min_clip(sample: f64, max: f64) -> f64 {
        return if sample < max { sample } else { max };
    }

    fn max_clip(sample: f64, max: f64) -> f64 {
        return if sample > -max { sample } else { -max };
    }
}