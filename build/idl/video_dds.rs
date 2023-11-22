#[derive(Debug, dust_dds::topic_definition::type_support::DdsType)]
pub struct Video {
    pub user_id: i16,
    pub frame_num: i32,
    pub frame: Vec<u8>,
}
