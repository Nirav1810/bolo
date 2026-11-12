//! Choosing the input device: the user's pick by name, or the system default.
//!
//! Pure, so the rules are tested without audio hardware; [`crate::capture`] feeds it the
//! devices that are plugged in right now, every time it opens a stream.

/// Which device to open for the name the user chose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// No choice (or a blank one): the system default.
    Default,
    /// The chosen device, at this index of the list.
    Device(usize),
    /// The chosen device isn't plugged in: fall back to the default.
    Missing,
}

/// The device in `available` (the input devices' names) to open for `wanted`. An exact
/// name wins; otherwise the first name that matches ignoring case and surrounding spaces.
pub fn pick<S: AsRef<str>>(wanted: Option<&str>, available: &[S]) -> Pick {
    let Some(wanted) = wanted.map(str::trim).filter(|w| !w.is_empty()) else { return Pick::Default };
    let exact = available.iter().position(|a| a.as_ref() == wanted);
    let loose = || available.iter().position(|a| a.as_ref().trim().to_lowercase() == wanted.to_lowercase());
    exact.or_else(loose).map_or(Pick::Missing, Pick::Device)
}

#[cfg(test)]
mod tests {
    use super::{Pick, pick};

    const DEVICES: [&str; 3] =
        ["Microphone (Realtek(R) Audio)", "Headset (Jabra Evolve2 65)", "headset (jabra evolve2 65)"];

    #[test]
    fn no_choice_or_a_blank_one_is_the_default() {
        assert_eq!(pick(None, &DEVICES), Pick::Default);
        assert_eq!(pick(Some("  "), &DEVICES), Pick::Default);
        assert_eq!(pick(None, &[] as &[&str]), Pick::Default);
    }

    #[test]
    fn the_chosen_device_is_found_by_name() {
        assert_eq!(pick(Some("Microphone (Realtek(R) Audio)"), &DEVICES), Pick::Device(0));
        assert_eq!(pick(Some(" microphone (realtek(r) audio) "), &DEVICES), Pick::Device(0), "case and spaces");
    }

    #[test]
    fn an_exact_name_beats_a_case_insensitive_one() {
        assert_eq!(pick(Some("headset (jabra evolve2 65)"), &DEVICES), Pick::Device(2));
        assert_eq!(pick(Some("HEADSET (JABRA EVOLVE2 65)"), &DEVICES), Pick::Device(1), "else the first match");
    }

    #[test]
    fn an_unplugged_device_falls_back_and_a_replugged_one_is_found_again() {
        let unplugged = [DEVICES[0]];
        assert_eq!(pick(Some(DEVICES[1]), &unplugged), Pick::Missing);
        assert_eq!(pick(Some(DEVICES[1]), &[] as &[&str]), Pick::Missing, "no devices at all");
        let replugged = [DEVICES[0], DEVICES[1]];
        assert_eq!(pick(Some(DEVICES[1]), &replugged), Pick::Device(1));
    }
}
