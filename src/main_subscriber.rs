use dust_dds::{
    domain::domain_participant_factory::DomainParticipantFactory,
    infrastructure::status::StatusKind,
    infrastructure::{listeners::NoOpListener, qos::QosKind, status::NO_STATUS},
    subscription::data_reader_listener::DataReaderListener,
    subscription::sample_info::{ANY_INSTANCE_STATE, ANY_SAMPLE_STATE, ANY_VIEW_STATE},
};
use gstreamer::prelude::*;

include!("../build/idl/video_dds.rs");

struct Listener {
    appsrc: gstreamer_app::AppSrc,
}

impl DataReaderListener for Listener {
    type Foo = Video;

    fn on_data_available(
        &mut self,
        the_reader: &dust_dds::subscription::data_reader::DataReader<Self::Foo>,
    ) {
        if let Ok(samples) =
            the_reader.read(1, ANY_SAMPLE_STATE, ANY_VIEW_STATE, ANY_INSTANCE_STATE)
        {
            for sample in samples {
                if let Ok(sample_data) = sample.data() {
                    println!("sample received: {:?}", sample_data.frame_num);

                    let mut buffer = gstreamer::Buffer::with_size(sample_data.frame.len()).unwrap();
                    {
                        let buffer_ref = buffer.get_mut().unwrap();
                        let mut buffer_samples = buffer_ref.map_writable().unwrap();
                        buffer_samples.clone_from_slice(sample_data.frame.as_slice());
                    }
                    self.appsrc.push_buffer(buffer).unwrap();

                    use std::io::{self, Write};
                    let _ = io::stdout().flush();
                }
            }
        }
    }
}

fn main() {
    gstreamer::init().unwrap();

    let domain_id = 0;
    let participant_factory = DomainParticipantFactory::get_instance();

    let participant = participant_factory
        .create_participant(domain_id, QosKind::Default, NoOpListener::new(), NO_STATUS)
        .unwrap();

    let topic = participant
        .create_topic(
            "VideoStream",
            "VideoStream",
            QosKind::Default,
            NoOpListener::new(),
            NO_STATUS,
        )
        .unwrap();

    let subscriber = participant
        .create_subscriber(QosKind::Default, NoOpListener::new(), NO_STATUS)
        .unwrap();

    let pipeline = gstreamer::parse_launch(
        "appsrc name=appsrc ! video/x-raw,format=RGB,width=160,height=90,framerate=10/1 ! videoconvert ! taginject tags=\"title=Subscriber\" ! autovideosink"
    )
    .unwrap();

    // Start playing
    pipeline
        .set_state(gstreamer::State::Playing)
        .expect("Unable to set the pipeline to the `Playing` state");

    let bin = pipeline.downcast_ref::<gstreamer::Bin>().unwrap();
    let appsrc_element = bin.by_name("appsrc").unwrap();
    let appsrc = appsrc_element.downcast::<gstreamer_app::AppSrc>().unwrap();

    let _reader = subscriber
        .create_datareader(
            &topic,
            QosKind::Default,
            Listener { appsrc },
            &[StatusKind::DataAvailable],
        )
        .unwrap();

    // Wait until error or EOS
    let bus = pipeline.bus().unwrap();
    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
        use gstreamer::MessageView;

        match msg.view() {
            MessageView::Eos(..) => break,
            MessageView::Error(err) => {
                println!(
                    "Error from {:?}: {} ({:?})",
                    err.src().map(|s| s.path_string()),
                    err.error(),
                    err.debug()
                );
                break;
            }
            _ => (),
        }
    }

    // Shutdown pipeline
    pipeline
        .set_state(gstreamer::State::Null)
        .expect("Unable to set the pipeline to the `Null` state");
}
