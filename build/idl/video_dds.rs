#[derive(Debug, serde::Deserialize, serde::Serialize, dust_dds::DdsType)]
pub struct Video {
    pub user_id: i16,
    pub frame_num: i32,
    pub frame: Vec<u8>,
}
