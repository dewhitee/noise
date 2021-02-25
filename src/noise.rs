pub mod noise {

    extern crate condition_variable;
    extern crate num;
    extern crate winapi;
    use num::pow;
    use num::Integer;

    use std::mem;
    use std::ptr;
    use std::sync::atomic::{AtomicBool, AtomicU32, AtomicPtr, Ordering};
    use std::sync::Mutex;
    use std::sync::Arc;
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

        condition_variable: condition_variable::ConditionVariable<usize>,
        mux_block_not_zero: Mutex<usize>,

        wave_headers: Vec<mmsystem::WAVEHDR>, // Array of headers
        hw_device: mmsystem::HWAVEOUT,        // Chosen device

        ready: AtomicBool,
        block_free: AtomicU32,
        thread: thread::JoinHandle<()>,
        global_time: f64,
    }

    impl Noise for NoiseMaker {
        fn user_process(&self, _: f64) -> f64 { 
            return 0.0;
        }
    }

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

                condition_variable: condition_variable::ConditionVariable::new(0),
                mux_block_not_zero: Mutex::new(0),

                wave_headers: Vec::<mmsystem::WAVEHDR>::new(),
                hw_device: ptr::null_mut(),

                ready: AtomicBool::new(false),
                thread: thread::spawn(|| {}),
                global_time: 0.0,
            };

            return obj;
        }

        pub unsafe fn create(&mut self, output_device: String) -> bool {
            //self.thread = thread::spawn(self.main_thread);
            let devices: Vec<String> = Self::enumerate();
            let mut devices_iter = devices.iter();
            println!("Chosen output device {}", output_device);
            let find_res = devices_iter.position(|x| x.eq(&output_device));

            let device_id: usize = match find_res {
                Some(x) => x,
                None => 0,
            };
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
                // Self::wave_out_proc_wrapper as basetsd::DWORD_PTR
                if mmeapi::waveOutOpen(&mut self.hw_device, device_id as u32, &wave_format, mem::zeroed(), 
                    self as *const Self as basetsd::DWORD_PTR, mmsystem::CALLBACK_FUNCTION) != winerror::S_OK as u32
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
            //self.wave_headers = vec![mmsystem::WAVEHDR::default(); self.block_count as u16];
            //let mut arr = [0; mem::size_of::<T>() * self.block_count * self.block_samples];
            println!("Reserving memory for Wave headers");
            //self.wave_headers.reserve(self.block_count as usize);
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

            //? Clone self and wrap into the mutex
            //let cloned = self.clone();
            //let cloned = Mutex::new(self);
            //let cloned = Arc::new(cloned);

            let atomic_ptr = AtomicPtr::new(self);

            //let thread_arc = cloned.clone();
            //? Starting thread
            self.thread = thread::spawn(move || {
                println!("Main thread running!");
                let noise = atomic_ptr.load(Ordering::SeqCst);
                (*noise).main_thread();
            });

            println!("Thread started!");

            //self.thread = thread::spawn(|| Self::main_thread(&mut self));

            //self.thread = thread::spawn(|| )

            //? Start
            println!("Created new mutex");
            Mutex::new(&self.mux_block_not_zero);
            println!("Notify one");
            self.condition_variable.touch(condition_variable::Notify::One);

            return true;
        }

        fn destroy(&self) -> bool {
            return false;
        }

        fn stop(mut self) {
            *self.ready.get_mut() = false;
            self.thread.join().unwrap();
        }

        pub fn get_time(&self) -> f64 {
            return self.global_time;
        }

        pub fn main_thread(&mut self) -> () {
            println!("Main thread running...");
            self.global_time = 0.0;
            let time_step: f64 = 1.0 / self.sample_rate as f64;
            
            let max_sample: i32 = pow(2, (std::mem::size_of::<u16>() * 8) - 1) - 1;
            
            let dmax_sample: f64 = max_sample as f64;
            let mut previous_sample = 0;
            
            println!("Time step = {}, Max sample = {}, Dmax sample = {}", time_step, max_sample, dmax_sample);
            while self.ready.load(Ordering::Relaxed) {
                // Wait for block to become available
                if self.block_free.load(Ordering::SeqCst) == 0 {
                    Mutex::new(&self.mux_block_not_zero);
                }

                // Block is here, so use it
                // todo: fix thread '<unnamed>' panicked at 'attempt to subtract with overflow', src\noise.rs:227:17
                *self.block_free.get_mut() -= 1;

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

                let mut new_sample;
                let current_block: u32 = self.block_current * self.block_samples;
                //println!("Current block = {}", current_block);

                for n in 0 .. self.block_samples {
                    println!("{}-th block sample", n);
                    // User process
                    if self.user_function.load(Ordering::SeqCst) == ptr::null_mut() {
                        //println!("Running user PROCESS");
                        new_sample = (self.clip(self.user_process(self.global_time), 1.0) * dmax_sample) as u16;
                    } else {
                        unsafe {
                            //println!("Running user FUNCTION | address = {}", (*self.user_function) as u32);
                            //new_sample = (self.clip((*self.user_function)(self.global_time), 1.0) * dmax_sample) as u16;
                            //println!("Running user FUNCTION | address = {}", (*self.user_function.load(Ordering::SeqCst)) as u32);
                            let f = *self.user_function.load(Ordering::SeqCst);
                            new_sample = (self.clip((f)(self.global_time), 1.0) * dmax_sample) as u16;
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
                self.block_current += 1;
                self.block_current %= self.block_count;
            }
        }

        pub fn wave_out_proc(
            mut self,
            wave_out: mmsystem::HWAVEOUT,
            msg: u32,
            dw_param1: minwindef::DWORD,
            dw_param2: minwindef::DWORD,
        ) {
            println!("Wave out proc");
            if msg != mmsystem::WOM_DONE {
                return;
            }

            *self.block_free.get_mut() += 1;
            Mutex::new(self.mux_block_not_zero);
            self.condition_variable
                .touch(condition_variable::Notify::One);
        }

        unsafe fn wave_out_proc_wrapper(
            wave_out: mmsystem::HWAVEOUT,
            msg: u32,
            dw_instance: minwindef::DWORD,
            dw_param1: minwindef::DWORD,
            dw_param2: minwindef::DWORD,
        ) {
            println!("Wave out process wrapper");
            (std::ptr::read(dw_instance as *mut NoiseMaker)).wave_out_proc(wave_out, msg, dw_param1, dw_param2);
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
}
