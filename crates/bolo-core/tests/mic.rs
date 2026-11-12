//! The input device (M4 ticket 03): the engine opens the chosen device, says once per
//! unplug that it fell back to the default, follows a re-plug, and replaces a stream
//! that failed instead of wedging. The "devices" are fakes; nothing touches real audio.

mod common;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bolo_core::{AudioFactory, AudioSource, EngineConfig, Language, LanguageMode, Polish, WavSource};
use common::{AUDIO_SECS, Ev, FakeStt, Harness, config, errors, finals, tone};

const DEFAULT: &str = "Microphone Array";
const USB: &str = "USB Headset";

/// The fake machine: which devices are plugged in, and every open.
#[derive(Default)]
struct Devices {
    plugged: Mutex<Vec<String>>,
    /// The device asked for at each open (`None`: the default).
    opens: Mutex<Vec<Option<String>>>,
    /// The newest source's "stream failed" switch.
    failed: Mutex<Arc<AtomicBool>>,
}

impl Devices {
    fn new(plugged: &[&str]) -> Arc<Self> {
        let d = Self::default();
        d.set_plugged(plugged);
        Arc::new(d)
    }

    fn set_plugged(&self, plugged: &[&str]) {
        *self.plugged.lock().unwrap() = plugged.iter().map(|s| s.to_string()).collect();
    }

    fn is_plugged(&self, name: &str) -> bool {
        self.plugged.lock().unwrap().iter().any(|p| p == name)
    }

    fn opens(&self) -> Vec<Option<String>> {
        self.opens.lock().unwrap().clone()
    }

    /// The open stream's device goes away mid-session.
    fn fail_stream(&self) {
        self.failed.lock().unwrap().store(true, Ordering::Relaxed);
    }

    fn factory(self: &Arc<Self>) -> AudioFactory {
        let devices = self.clone();
        Arc::new(move |wanted| {
            devices.opens.lock().unwrap().push(wanted.map(str::to_owned));
            let fell_back = wanted.is_some_and(|w| !devices.is_plugged(w));
            let failed = Arc::new(AtomicBool::new(false));
            *devices.failed.lock().unwrap() = failed.clone();
            let device = if fell_back { DEFAULT } else { wanted.unwrap_or(DEFAULT) };
            Ok(Box::new(FakeMic {
                audio: WavSource::new(tone(AUDIO_SECS)),
                device: device.into(),
                wanted: wanted.map(str::to_owned),
                fell_back,
                failed,
                devices: devices.clone(),
            }) as Box<dyn AudioSource>)
        })
    }
}

struct FakeMic {
    audio: WavSource,
    device: String,
    wanted: Option<String>,
    fell_back: bool,
    failed: Arc<AtomicBool>,
    devices: Arc<Devices>,
}

impl AudioSource for FakeMic {
    fn sample_rate(&self) -> u32 {
        self.audio.sample_rate()
    }
    fn drain_mono(&mut self, out: &mut Vec<f32>) {
        if !self.failed.load(Ordering::Relaxed) {
            self.audio.drain_mono(out);
        }
    }
    fn level(&self) -> f32 {
        self.audio.level()
    }
    fn device_name(&self) -> String {
        self.device.clone()
    }
    fn fell_back(&self) -> bool {
        self.fell_back
    }
    fn failed(&self) -> bool {
        self.failed.load(Ordering::Relaxed)
    }
    fn wanted_is_back(&self) -> bool {
        self.wanted.as_deref().is_some_and(|w| self.devices.is_plugged(w))
    }
}

fn stt() -> FakeStt {
    FakeStt::new().script(Language::En, &["hello"], "hello there")
}

/// An engine choosing `device`, its mic kept warm for `warm_secs`.
fn harness(devices: &Arc<Devices>, device: Option<&str>, warm_secs: u32) -> Harness {
    let config = EngineConfig {
        input_device: device.map(str::to_owned),
        mic_warm_secs: warm_secs,
        ..config(LanguageMode::English, Polish::Off)
    };
    Harness::with_factory(stt(), config, devices.factory())
}

fn fallbacks(events: &[Ev]) -> Vec<(String, String)> {
    events
        .iter()
        .filter_map(|e| if let Ev::MicFallback(w, u) = e { Some((w.clone(), u.clone())) } else { None })
        .collect()
}

/// A dictation went through: no error and the text came.
fn assert_dictated(events: &[Ev]) {
    assert!(errors(events).is_empty(), "{events:?}");
    assert_eq!(finals(events).len(), 1, "{events:?}");
}

#[test]
fn the_chosen_device_is_opened() {
    let devices = Devices::new(&[DEFAULT, USB]);
    let h = harness(&devices, Some(USB), 0);
    let events = h.dictate();
    assert_dictated(&events);
    assert!(fallbacks(&events).is_empty());
    assert_eq!(devices.opens(), [Some(USB.to_owned())]);
}

#[test]
fn an_unplugged_choice_falls_back_with_one_notice_per_unplug() {
    let devices = Devices::new(&[DEFAULT]);
    let h = harness(&devices, Some(USB), 0);

    let first = h.dictate();
    assert_dictated(&first);
    assert_eq!(fallbacks(&first), [(USB.to_owned(), DEFAULT.to_owned())]);
    let second = h.dictate();
    assert_dictated(&second);
    assert!(fallbacks(&second).is_empty(), "not every dictation: {second:?}");

    devices.set_plugged(&[DEFAULT, USB]);
    let replugged = h.dictate();
    assert_dictated(&replugged);
    assert!(fallbacks(&replugged).is_empty());

    devices.set_plugged(&[DEFAULT]);
    let again = h.dictate();
    assert_eq!(fallbacks(&again).len(), 1, "a new unplug is told again: {again:?}");
    assert_eq!(devices.opens(), vec![Some(USB.to_owned()); 4], "every open asks for the choice");
}

#[test]
fn a_stream_that_fails_mid_session_is_replaced_by_the_next_dictation() {
    let devices = Devices::new(&[DEFAULT, USB]);
    let h = harness(&devices, Some(USB), 120);
    assert_dictated(&h.dictate());
    assert_eq!(devices.opens().len(), 1, "warm: one open");

    // Unplugged while warm and idle: the mic closes, and the next dictation falls back.
    devices.set_plugged(&[DEFAULT]);
    devices.fail_stream();
    std::thread::sleep(Duration::from_millis(100));
    let after = h.dictate();
    assert_dictated(&after);
    assert_eq!(fallbacks(&after).len(), 1, "{after:?}");
    assert_eq!(devices.opens().len(), 2);

    // Failing during a dictation: that one ends (without audio), the next one reopens.
    h.engine.start_dictation(Default::default()).unwrap();
    devices.fail_stream();
    std::thread::sleep(Duration::from_millis(100));
    h.engine.stop_dictation().unwrap();
    let _ = h.events_until_idle();
    assert_dictated(&h.dictate());
    assert_eq!(devices.opens().len(), 3);
}

#[test]
fn a_warm_mic_moves_to_a_new_choice_and_back_to_a_replugged_one() {
    let devices = Devices::new(&[DEFAULT]);
    let h = harness(&devices, None, 120);
    assert_dictated(&h.dictate());
    assert_eq!(devices.opens(), [None]);

    // A new choice reopens the warm mic at once (the model isn't touched).
    h.engine.set_input_device(Some(USB.into()));
    let events = h.events_within(Duration::from_millis(300));
    assert_eq!(devices.opens(), [None, Some(USB.to_owned())]);
    assert_eq!(fallbacks(&events), [(USB.to_owned(), DEFAULT.to_owned())], "it isn't plugged in");

    // Plugged in while the warm mic is on the default: found within a few seconds.
    devices.set_plugged(&[DEFAULT, USB]);
    let _ = h.events_within(Duration::from_millis(3500));
    assert_eq!(devices.opens().len(), 3, "{:?}", devices.opens());
    assert_dictated(&h.dictate());
    assert_eq!(devices.opens().len(), 3, "still warm on the re-plugged device");
}
