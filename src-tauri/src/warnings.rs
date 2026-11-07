use std::sync::Mutex;

// Ancillary failures must not turn a committed filesystem operation into an
// apparent failure. Return the actual result and surface these to the UI.
static WARNINGS: Mutex<Vec<String>> = Mutex::new(Vec::new());
pub fn push(message: &str) {
    if let Ok(mut values) = WARNINGS.lock() {
        if values.len() < 20 && !values.iter().any(|v| v == message) {
            values.push(message.into());
        }
    }
}
pub fn take() -> Vec<String> {
    WARNINGS
        .lock()
        .map(|mut values| std::mem::take(&mut *values))
        .unwrap_or_default()
}
