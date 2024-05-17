use dust_dds::{
    domain::domain_participant_factory::DomainParticipantFactory,
    infrastructure::{
        error::DdsError,
        qos::QosKind,
        status::{StatusKind, NO_STATUS},
    },
    subscription::{
        data_reader_listener::DataReaderListener,
        sample_info::{ANY_INSTANCE_STATE, ANY_SAMPLE_STATE, ANY_VIEW_STATE},
    },
};
use gstreamer::prelude::*;

include!("../target/idl/video_dds.rs");

#[derive(Debug)]
struct Error(String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl From<gstreamer::glib::Error> for Error {
    fn from(value: gstreamer::glib::Error) -> Self {
        Self(format!("GStreamer error: {}", value))
    }
}
impl From<gstreamer::StateChangeError> for Error {
    fn from(value: gstreamer::StateChangeError) -> Self {
        Self(format!("GStreamer state change error: {}", value))
    }
}
impl From<&gstreamer::message::Error> for Error {
    fn from(value: &gstreamer::message::Error) -> Self {
        Self(format!("GStreamer state change error: {:?}", value))
    }
}
impl From<DdsError> for Error {
    fn from(value: DdsError) -> Self {
        Self(format!("DDS error: {:?}", value))
    }
}

struct Listener {
    appsrc: gstreamer_app::AppSrc,
}

impl DataReaderListener for Listener {
    type Foo = Video;

    fn on_data_available(
        &mut self,
        the_reader: dust_dds::subscription::data_reader::DataReader<Self::Foo>,
    ) {
        if let Ok(samples) =
            the_reader.read(1, ANY_SAMPLE_STATE, ANY_VIEW_STATE, ANY_INSTANCE_STATE)
        {
            for sample in samples {
                if let Ok(sample_data) = sample.data() {
                    println!("sample received: {:?}", sample_data.frame_num);

                    let mut buffer = gstreamer::Buffer::with_size(sample_data.frame.len())
                        .expect("buffer creation failed");
                    {
                        let buffer_ref = buffer.get_mut().expect("Buffer not writable");
                        let mut buffer_samples =
                            buffer_ref.map_writable().expect("Buffer not mappable");
                        buffer_samples.clone_from_slice(sample_data.frame.as_slice());
                    }
                    self.appsrc
                        .push_buffer(buffer)
                        .expect("Failed pushing buffer into appsrc");

                    use std::io::{self, Write};
                    io::stdout().flush().ok();
                }
            }
        }
    }
}

fn main() -> Result<(), Error> {
    gstreamer::init()?;

    let domain_id = 0;
    let participant_factory = DomainParticipantFactory::get_instance();
    let participant =
        participant_factory.create_participant(domain_id, QosKind::Default, None, NO_STATUS)?;
    let topic = participant.create_topic::<Video>(
        "VideoStream",
        "VideoStream",
        QosKind::Default,
        None,
        NO_STATUS,
    )?;
    let subscriber = participant.create_subscriber(QosKind::Default, None, NO_STATUS)?;

    let pipeline = gstreamer::parse_launch(
        r#"appsrc name=appsrc ! openh264dec ! videoconvert ! taginject tags="title=Subscriber" ! autovideosink"#,
    )?;

    pipeline.set_state(gstreamer::State::Playing)?;

    let bin = pipeline.downcast_ref::<gstreamer::Bin>().expect("Pipeline must be bin");
    let appsrc_element = bin.by_name("appsrc").expect("Pipeline must have appsrc");
    let appsrc = appsrc_element.downcast::<gstreamer_app::AppSrc>().expect("appsrc must be an AppSrc");
    let src_caps = gstreamer::Caps::builder("video/x-h264")
        .field("stream-format", "byte-stream")
        .field("alignment", "au")
        .field("profile", "constrained-baseline")
        .build();
    appsrc.set_caps(Some(&src_caps));

    let _reader = subscriber.create_datareader(
        &topic,
        QosKind::Default,
        Some(Box::new(Listener { appsrc })),
        &[StatusKind::DataAvailable],
    )?;

    // Wait until error or EOS
    let bus = pipeline.bus().expect("Pipeline must have bus");
    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
        match msg.view() {
            gstreamer::MessageView::Eos(..) => break,
            gstreamer::MessageView::Error(err) => Err(err)?,
            _ => (),
        }
    }

    pipeline.set_state(gstreamer::State::Null)?;

    Ok(())
}
