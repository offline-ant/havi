//! pylon CLI binary.

fn main() {
    env_logger::init();
    pylon::cli::main(std::env::args().skip(1).collect());
}
