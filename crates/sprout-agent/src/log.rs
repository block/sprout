macro_rules! log_info {
    ($($arg:tt)*) => { eprintln!("[INFO] sprout-agent: {}", format_args!($($arg)*)) };
}

macro_rules! log_warn {
    ($($arg:tt)*) => { eprintln!("[WARN] sprout-agent: {}", format_args!($($arg)*)) };
}

macro_rules! log_error {
    ($($arg:tt)*) => { eprintln!("[ERROR] sprout-agent: {}", format_args!($($arg)*)) };
}

pub(crate) use log_error;
pub(crate) use log_info;
pub(crate) use log_warn;
