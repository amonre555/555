pub mod deezer;
pub mod local;
pub mod youtube;

pub use deezer::DeezerSource;
pub use local::{read_cover, LocalLibrary};
pub use youtube::YouTubeMusicSource;
