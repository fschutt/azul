//! The simulator of the AzReview phone companion: a window showing the phone's screens at the size of a phone beside a
//! panel that plays the Mac. `companion_sim --help` says how to pick the phone, the language and the screen.

fn main() {
    azul_companion::run_sim(std::env::args().skip(1).collect());
}
