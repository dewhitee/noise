pub mod noise {

    //extern crate condition_variable;
    extern crate num;
    extern crate winapi;

    use crate::envelope::envelope::EnvelopeADSR;

    use num::pow;
    use num::Integer;

    use std::mem;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicPtr, Ordering};
    use std::sync::{Mutex, Arc, Condvar, Weak};
    use std::thread;
    use std::vec::Vec;
    use std::clone::Clone;

    use winapi::shared::minwindef;
    use winapi::shared::mmreg;
    use winapi::shared::winerror;
    use winapi::shared::basetsd;
    use winapi::shared::ntdef;
    use winapi::um::mmeapi;
    use winapi::um::mmsystem;

    use std::f64::consts::PI;

    pub struct NoiseArgs {
        pub sample_rate: u32,
        pub channels: u32,
        pub blocks: u32,
        pub block_samples: u32,
    }

    impl Default for NoiseArgs {
        fn default() -> Self {
            Self {
                sample_rate: 44100,
                channels: 1,
                blocks: 8,
                block_samples: 512,
            }
        }
    }

    trait Noise {
        fn user_process(&self, delta_time: f64) -> f64;
    }

    pub struct NoiseMaker {
        user_function: AtomicPtr<fn(f64) -> f64>,
        sample_rate: u32,
        channels: u32,
        block_count: u32,
        block_samples: u32,
        block_current: u32,
        block_memory: Vec<i16>,

        condition_variable: Condvar,
        mux_block_not_zero: Mutex<i16>,

        wave_headers: Vec<mmsystem::WAVEHDR>, // Array of headers
        hw_device: mmsystem::HWAVEOUT,        // Chosen device

        ready: AtomicBool,
        block_free: AtomicU32,
        thread: thread::JoinHandle<()>,
        global_time: f64,

        frequency_output: f64,
        envelop: AtomicPtr<EnvelopeADSR>
    }

    impl Noise for NoiseMaker {
        fn user_process(&self, _: f64) -> f64 { 
            return 0.0;
        }
    }

    impl Drop for NoiseMaker {
     
        fn drop(&mut self) { self.destroy(); }
    }

    static mut ATOMIC_PTR: AtomicPtr<NoiseMaker> = AtomicPtr::<NoiseMaker>::new(ptr::null_mut());
    //static mut ENVELOPE: AtomicPtr<EnvelopeADSR> = AtomicPtr::<EnvelopeADSR>::new(ptr::null_mut());
    //static mut FREQUENCY_OUTPUT: AtomicPtr<f64> = AtomicPtr::<f64>::new(ptr::null_mut());

    impl NoiseMaker {
        pub fn new(args: NoiseArgs) -> Self {
            let obj = Self {
                user_function: AtomicPtr::<fn(f64) -> f64>::new(ptr::null_mut()),
                sample_rate: args.sample_rate,
                channels: args.channels,

                block_count: args.blocks,
                block_samples: args.block_samples,
                block_current: 0,
                block_memory: Vec::<i16>::new(),
                block_free: AtomicU32::new(args.blocks),

                condition_variable: Condvar::new(),
                mux_block_not_zero: Mutex::new(0),

                wave_headers: Vec::<mmsystem::WAVEHDR>::new(),
                hw_device: ptr::null_mut(),

                ready: AtomicBool::new(false),
                thread: thread::spawn(|| {}),
                global_time: 0.0,

                frequency_output: 0.0,
                envelop: AtomicPtr::new(&mut EnvelopeADSR::new()),
            };

            return obj;
        }

        pub fn set_frequency_output(&mut self, frequency: f64) {
            self.frequency_output = frequency;
        }

        pub fn get_frequency_output(&self) -> f64 {
            return self.frequency_output;
        }

        pub fn set_envelop_note_on(&mut self) {
            println!("Set envelop note on");
            let val = self.envelop.load(Ordering::SeqCst);
            unsafe {
                (*val).set_note_on(self.get_time());
            }
        }

        pub fn set_envelope_note_off(&mut self) {
            println!("Set envelop note off");
            let val = self.envelop.load(Ordering::SeqCst);
            unsafe {
                (*val).set_note_off(self.get_time());
            }
        }

        pub unsafe fn create(&mut self, output_device: String) -> bool {
            let devices: Vec<String> = Self::enumerate();
            let mut devices_iter = devices.iter();
            println!("Chosen output device {}", output_device);
            let find_res = devices_iter.position(|x| x.eq(&output_device));

            let device_id: usize = match find_res {
                Some(x) => x,
                None => 0,
            };

            ATOMIC_PTR = AtomicPtr::new(self);
            //ENVELOPE = AtomicPtr::new(&mut EnvelopeADSR::new());

            println!("Device id {} | devices len {}", device_id, devices.len());
            if device_id < devices.len() {
                println!("Initializing wave_format");
                // Device is available
                let mut wave_format: mmreg::WAVEFORMATEX = mem::zeroed::<mmreg::WAVEFORMATEX>();
                wave_format.wFormatTag = mmreg::WAVE_FORMAT_PCM;
                wave_format.nSamplesPerSec = self.sample_rate;
                wave_format.wBitsPerSample = (mem::size_of::<u16>() * 8) as u16;
                wave_format.nChannels = self.channels as u16;
                wave_format.nBlockAlign = (wave_format.wBitsPerSample / 8) * wave_format.nChannels;
                wave_format.nAvgBytesPerSec = wave_format.nSamplesPerSec * wave_format.nBlockAlign as u32;
                wave_format.cbSize = 0;

                // Open Device if valid
                println!("Opening device (waveOutOpen)");
                let selfptr = ATOMIC_PTR.load(Ordering::SeqCst) as basetsd::DWORD_PTR;
                let callback_func_ptr = Self::wave_out_proc_wrapper as basetsd::DWORD_PTR;
                println!("Pointer to self as basetsd::DWORD_PTR = {}", selfptr);
                println!("Pointer to self as u32 = {}", ATOMIC_PTR.load(Ordering::SeqCst) as u32);
                if mmeapi::waveOutOpen(&mut self.hw_device, device_id as u32, &wave_format, callback_func_ptr, 
                    selfptr, mmsystem::CALLBACK_FUNCTION) != winerror::S_OK as u32
                {
                    println!("Failed to open");
                    return self.destroy();
                }

                println!("Wave format initialized!");
            }

            // Allocate Wave | Block memory
            println!("Allocating wave (block count = {}, block_samples = {})", self.block_count, self.block_samples);
            let block_memory_len = self.block_count * self.block_samples;
            self.block_memory = vec![0; block_memory_len as usize];
            println!("block memory len = {} | ({})", self.block_memory.len(), block_memory_len);
            println!("Reserving memory for Wave headers");
            self.wave_headers = vec![mem::zeroed(); self.block_count as usize];
            println!("Wave headers len = {}", self.wave_headers.len());

            // Link headers to block memory
            println!("Linking headers to block memory");
            for n in 0..self.block_count {
                let dw_buffer_length = self.block_samples * mem::size_of::<u16>() as u32;
                let lp_data = ((self.block_memory).as_ptr().offset((n * self.block_samples) as isize)) as ntdef::LPSTR;

                println!("Linking {}-th header | dwBufferLength = {} | wave headers len = {}", n, dw_buffer_length, self.wave_headers.len());
                self.wave_headers[n as usize].dwBufferLength = dw_buffer_length;
                self.wave_headers[n as usize].lpData = lp_data;
            }

            *self.ready.get_mut() = true;

            println!("atomic_ptr before thread = {}", ATOMIC_PTR.load(Ordering::SeqCst) as basetsd::DWORD_PTR);

            //? Starting thread
            self.thread = thread::spawn(move || {
                println!("Main thread running!");
                let noise = ATOMIC_PTR.load(Ordering::SeqCst);
                println!("atomic_ptr = {}", noise as basetsd::DWORD_PTR);
                (*noise).main_thread();
                println!("Thread passed");
            });

            println!("Thread started!");

            //? Start

            println!("Created new mutex");
            let mutex = Mutex::new(&self.mux_block_not_zero);
            let started = mutex.lock().unwrap();

            println!("Notify one");
            self.condition_variable.notify_one();

            return true;
        }

        fn destroy(&self) -> bool {
            return false;
        }

        // fn stop(&mut self) {
        //     *self.ready.get_mut() = false;
        //     self.thread.join().unwrap();
        // }

        pub fn get_time(&self) -> f64 {
            return self.global_time;
        }

        fn oscillate(&self, hertz: f64, current_time: f64, osc_type: i32) -> f64 {
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

        unsafe fn default_make_noise(&self, current_time: f64) -> f64 {
            //let output: f64 = 
            //println!("Making noise!!");
            return 0.5 * f64::sin(self.frequency_output * 2 as f64 * PI * current_time);
            //println!("Freq output = {} | oscillate = {}", self.frequency_output, self.oscillate(self.frequency_output * 0.5, current_time, 3));
            //let envelope = self.envelop.load(Ordering::SeqCst);
            //let output = (*envelope).get_amplitude(current_time) *
            //(
            //    self.oscillate(self.frequency_output * 0.5, current_time, 3) + 
            //    self.oscillate(self.frequency_output * 1.0, current_time, 1)
            //);
            //println!("output = {} | Freq output = {} | oscillate = {} | amplitude = {}", output, self.frequency_output, self.oscillate(self.frequency_output * 0.5, current_time, 3), 
            //(*envelope).get_amplitude(current_time));
            //println!("output = {}", output);
            //return output * 0.4;
        }

        pub fn main_thread(&mut self) -> () {
            println!("Main thread running...");
            self.global_time = 0.0;
            let time_step: f64 = 1.0 / self.sample_rate as f64;
            
            let max_sample: i32 = pow(2, (std::mem::size_of::<u16>() * 8) - 1) - 1;
            
            let dmax_sample: f64 = max_sample as f64;
            let mut previous_sample = 0;
            
            //println!("Time step = {}, Max sample = {}, Dmax sample = {}", time_step, max_sample, dmax_sample);
            while self.ready.load(Ordering::SeqCst) {
                //println!("main thread ready...");
                // Wait for block to become available
                if self.block_free.load(Ordering::SeqCst) == 0 {
                    //println!("Waiting for block to become available");
                    let mutex = Mutex::new(&self.mux_block_not_zero);
                    let started = mutex.lock().unwrap();
                    self.condition_variable.wait(started).unwrap();
                }
                //println!("block is here");
                // Block is here, so use it
                // todo: fix thread '<unnamed>' panicked at 'attempt to subtract with overflow', src\noise.rs:227:17
                *self.block_free.get_mut() -= 1;
                //println!("prepare block for processing");
                // Prepare block for processing
                if self.wave_headers[self.block_current as usize].dwFlags & 0x00000002 != 0
                {
                    //println!("Waving out unprepared header");
                    unsafe {
                        mmeapi::waveOutUnprepareHeader(
                            self.hw_device,
                            &mut self.wave_headers[self.block_current as usize],
                            mem::size_of::<mmsystem::WAVEHDR>() as u32,
                        );
                    }
                }
                //println!("new sample");
                let mut new_sample: i16;
                let current_block: u32 = self.block_current * self.block_samples;
                //println!("block_samples = {}", self.block_samples);
                for n in 0 .. self.block_samples {
                    //println!("Processing block");
                    // User process
                    if self.user_function.load(Ordering::SeqCst) == ptr::null_mut() {
                        //println!("User process");
                        new_sample = (self.clip(self.user_process(self.global_time), 1.0) * dmax_sample) as i16;
                    } else {
                        unsafe {
                            //println!("user function is loaded");
                            let f = *self.user_function.load(Ordering::SeqCst);
                            //println!("user function is loaded 2");
                            //println!("f = {}", f as u32);

                            //? This line triggers segfault when main loop is running with some code
                            //new_sample = (self.clip((f)(self.global_time), 1.0) * dmax_sample) as i16;
                            new_sample = (self.default_make_noise(self.global_time) * dmax_sample) as i16;
                            //println!("user function is loaded 3");
                        }
                    }

                    //println!("block memory len = {}, current block = {}, n = {}, new_sample = {}", self.block_memory.len(), current_block, n, new_sample);
                    self.block_memory[(current_block + n) as usize] = new_sample as i16;
                    previous_sample = new_sample;

                    self.global_time += time_step;
                    //println!("global_time is {} at n = {}", self.global_time, n);
                }

                // Send block to sound devices
                //println!("Sending block to sound devices");
                unsafe {
                    mmeapi::waveOutPrepareHeader(self.hw_device, &mut self.wave_headers[self.block_current as usize], mem::size_of::<mmsystem::WAVEHDR>() as u32);
                    mmeapi::waveOutWrite(self.hw_device, &mut self.wave_headers[self.block_current as usize], mem::size_of::<mmsystem::WAVEHDR>() as u32);
                }
                //println!("Blocks send");
                self.block_current += 1;
                //println!("block_current = {} | new block_current = {} (block_count = {})", self.block_current, self.block_current % self.block_count, self.block_count);
                self.block_current %= self.block_count;
            }
        }

        pub fn wave_out_proc(
            &mut self,
            wave_out: mmsystem::HWAVEOUT,
            msg: u32,
            dw_param1: minwindef::DWORD,
            dw_param2: minwindef::DWORD
        ) {
            //println!("Wave out proc");
            if msg != mmsystem::WOM_DONE {
                //println!("msg is not mmsystem::WOM_DONE!");
                return;
            } else {
                //println!("msg is mmsystem::WOM_DONE");
            }
            //println!("msg = {}", msg);
            unsafe {
                let p = ATOMIC_PTR.load(Ordering::SeqCst);
                //println!("hey = {}", p as u32);
                *(*p).block_free.get_mut() += 1;
                //println!("init mutex");
                let mutex = Mutex::new(&(*p).mux_block_not_zero);
                //println!("_started...");
                let _started = mutex.lock().unwrap();
                //println!("notifying...");
                (*p).condition_variable.notify_one();
            }
        }

        unsafe fn wave_out_proc_wrapper(
            wave_out: mmsystem::HWAVEOUT,
            msg: u32,
            dw_instance: minwindef::DWORD,
            dw_param1: minwindef::DWORD,
            dw_param2: minwindef::DWORD,
        ) {
            //println!("Wave out process wrapper | wave_out = {} | msg = {} | dw_instance = {} | dw_param1 = {} | dw_param2 = {}", wave_out as u32, msg, dw_instance, 
            //dw_param1 as u32, dw_param2 as u32);
            //println!("casting dw_instance as NoiseMaker (dw_instance = {})", dw_instance);
            let dw_instance_ptr: *mut NoiseMaker = dw_instance as *mut NoiseMaker;
            //println!("Calling wave_out_proc");
            (*dw_instance_ptr).wave_out_proc(wave_out, msg, dw_param1, dw_param2);
        }

        pub unsafe fn enumerate() -> Vec<String> {
            let device_count: u32 = mmeapi::waveOutGetNumDevs();
            let mut devices: Vec<String> = Vec::new();
            let mut woc: mmsystem::WAVEOUTCAPSW = mem::zeroed::<mmsystem::WAVEOUTCAPSW>();

            for n in 0..device_count {
                if mmeapi::waveOutGetDevCapsW(
                    n as usize,
                    &mut woc,
                    mem::size_of::<mmsystem::WAVEOUTCAPSW>() as u32,
                ) == winerror::S_OK as u32
                {
                    // todo: test
                    devices.push(String::from_utf16(&woc.szPname.to_owned()).unwrap());
                }
            }

            return devices;
        }

        pub fn set_user_function(&mut self, mut func: fn(f64) -> f64) {
            //self.user_function = &mut func;
            self.user_function.store(&mut func, Ordering::SeqCst);
            println!("User function is set!");
        }

        pub fn clip(&self, sample: f64, max: f64) -> f64 {
            //println!("clip sample = {}, max = {}", sample, max);
            return if sample >= 0.0 {
                min_clip(sample, max)
            } else {
                max_clip(sample, -max)
            };
        }
    }

    fn min_clip(sample: f64, max: f64) -> f64 {
        return if sample < max { sample } else { max };
    }

    fn max_clip(sample: f64, max: f64) -> f64 {
        return if sample > -max { sample } else { -max };
    }

    fn hz_to_angular(hertz: f64) -> f64 {
       return hertz * 2.0 * PI;
    }
}
