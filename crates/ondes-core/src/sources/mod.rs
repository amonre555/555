pub mod audius;
pub mod local;
pub mod youtube;

pub use audius::AudiusSource;
pub use local::{read_cover, LocalLibrary};
pub use youtube::YouTubeMusicSource;
