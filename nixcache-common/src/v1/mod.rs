pub mod header {
    /// Header containing the upload info.
    pub const NAR_INFO: &str = "X-Nixcache-Nar-Info";

    /// Header containing the size of the upload info at the beginning of the body.
    pub const NAR_INFO_PREAMBLE_SIZE: &str = "X-Nixcache-Nar-Info-Preamble-Size";
}

pub mod upload_path;
