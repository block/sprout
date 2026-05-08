macro_rules! log_warn {
    ($($arg:tt)*) => { eprintln!("[WARN] sprout-dev-mcp: {}", format_args!($($arg)*)) };
}

macro_rules! log_error {
    ($($arg:tt)*) => { eprintln!("[ERROR] sprout-dev-mcp: {}", format_args!($($arg)*)) };
}

pub(crate) use log_error;
pub(crate) use log_warn;
