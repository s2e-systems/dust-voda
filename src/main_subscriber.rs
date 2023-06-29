use dust_dds::{
    infrastructure::{
        qos::DataReaderQos,
        qos_policy::{ReliabilityQosPolicy, ReliabilityQosPolicyKind},
        time::{Duration, DurationKind}, status::StatusKind,
    },
    subscription::data_reader_listener::DataReaderListener,
};
use gstreamer::prelude::*;

use dust_dds::{
    domain::domain_participant_factory::DomainParticipantFactory,
    infrastructure::{qos::QosKind, status::NO_STATUS},
    subscription::sample_info::{ANY_INSTANCE_STATE, ANY_SAMPLE_STATE, ANY_VIEW_STATE},
    topic_definition::type_support::DdsType,
};

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, DdsType, Debug)]
struct Video {
    #[key]
    userid: i16,
    frameNum: i32,
    frame: Vec<u8>,
}
struct Listener {
    appsrc: gstreamer_app::AppSrc,
}

impl DataReaderListener for Listener {
    type Foo = Video;

    fn on_data_available(
        &mut self,
        the_reader: &dust_dds::subscription::data_reader::DataReader<Self::Foo>,
    ) {
        println!("on data available");
        if let Ok(samples) =
            the_reader.read(1, ANY_SAMPLE_STATE, ANY_VIEW_STATE, ANY_INSTANCE_STATE)
        {
            for sample in samples {
                let sample_data = sample.data.as_ref().unwrap();
                println!("sample received: {:?}", sample_data.frameNum);

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

fn main() {
    gstreamer::init().unwrap();

    let domain_id = 0;
    let participant_factory = DomainParticipantFactory::get_instance();

    let participant = participant_factory
        .create_participant(domain_id, QosKind::Default, None, NO_STATUS)
        .unwrap();

    let topic = participant
        .create_topic::<Video>("VideoStream", QosKind::Default, None, NO_STATUS)
        .unwrap();

    let subscriber = participant
        .create_subscriber(QosKind::Default, None, NO_STATUS)
        .unwrap();
    let reader_qos = DataReaderQos {
        reliability: ReliabilityQosPolicy {
            kind: ReliabilityQosPolicyKind::Reliable,
            max_blocking_time: DurationKind::Infinite,
        },
        ..Default::default()
    };

    let pipeline = gstreamer::parse_launch(&format!(
        "appsrc name=appsrc ! video/x-raw,format=RGB,width=160,height=90,framerate=10/1 ! videoconvert ! autovideosink"
    ))
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
            Some(Box::new(Listener { appsrc })),
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
