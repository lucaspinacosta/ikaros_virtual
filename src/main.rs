pub mod collectors;
pub mod companion;
pub mod context;
pub mod events;
mod overlay;
pub mod status;

fn main() {
    if std::env::args().any(|argument| argument == "--status") {
        overlay::run_status_panel();
    } else {
        overlay::run();
    }
}
