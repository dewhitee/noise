
pub mod envelope {
    pub struct EnvelopeADSR {
        attack_time: f64,
        decay_time: f64,
        release_time: f64,
        
        sustain_amplitude: f64,
        start_amplitude: f64,
        
        trigger_on_time: f64,
        trigger_off_time: f64,

        note_on: bool
    }

    impl EnvelopeADSR {
        pub fn new() -> Self {
            return Self {
                attack_time: 0.01,
                decay_time: 0.01,
                release_time: 0.02,

                sustain_amplitude: 0.8,
                start_amplitude: 1.0,

                trigger_on_time: 0.0,
                trigger_off_time: 0.0,

                note_on: false
            };
        }

        pub fn get_amplitude(&self, time: f64) -> f64 {
            let mut amplitude: f64 = 0.0;
            let lifetime: f64 = time - self.trigger_on_time;

            if self.note_on {
                // Attack, Decay, Sustain

                // Attack
                if lifetime <= self.attack_time {
                    amplitude = (lifetime / self.attack_time) * self.start_amplitude;
                }

                // Decay
                if lifetime > self.attack_time && lifetime <= (self.attack_time + self.decay_time) {
                    amplitude = ((lifetime - self.attack_time) / self.decay_time) * 
                    (self.sustain_amplitude - self.start_amplitude) + self.start_amplitude;
                }

                // Sustain
                if lifetime > (self.attack_time + self.decay_time) {
                    amplitude = self.sustain_amplitude;
                }

            } else {
                // Release
                amplitude = ((time - self.trigger_off_time) / self.release_time) * (0.0 - self.sustain_amplitude) + self.sustain_amplitude;
            }

            return amplitude;
        }

        pub fn set_note_on(&mut self, time_on: f64) {
            self.trigger_on_time = time_on;
            self.note_on = true;
        }

        pub fn set_note_off(&mut self, time_off: f64) {
            self.trigger_off_time = time_off;
            self.note_on = false;
        }
    }
}