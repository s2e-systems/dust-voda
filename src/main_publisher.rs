use dust_dds::{
    domain::domain_participant_factory::DomainParticipantFactory,
    infrastructure::{qos::QosKind, status::NO_STATUS},
};
use gstreamer::prelude::*;

include!("../build/idl/video_dds.rs");

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

    let publisher = participant
        .create_publisher(QosKind::Default, None, NO_STATUS)
        .unwrap();

    let writer = publisher
        .create_datawriter(&topic, QosKind::Default, None, NO_STATUS)
        .unwrap();

    let pipeline = gstreamer::parse_launch(
        "videotestsrc horizontal-speed=1 ! video/x-raw,format=RGB,width=160,height=90,framerate=10/1 ! tee name=t ! queue ! appsink name=appsink  t. ! queue ! videoconvert ! taginject tags=\"title=Publisher\" ! autovideosink"
    )
    .unwrap();

    // Start playing
    pipeline
        .set_state(gstreamer::State::Playing)
        .expect("Unable to set the pipeline to the `Playing` state");

    let bin = pipeline.downcast_ref::<gstreamer::Bin>().unwrap();
    let appsink_element = bin.by_name("appsink").unwrap();
    let appsink = appsink_element
        .downcast_ref::<gstreamer_app::AppSink>()
        .unwrap();

    let mut i = 0;
    appsink.set_callbacks(
        gstreamer_app::AppSinkCallbacks::builder()
            .new_sample(move |s| {
                if let Ok(sample) = s.pull_sample() {
                    let b = sample.buffer().unwrap().map_readable().unwrap();
                    let bytes = b.as_slice();

                    let video_sample = Video {
                        user_id: 8,
                        frame_num: i,
                        frame: bytes.to_vec(),
                    };
                    writer.write(&video_sample, None).unwrap();
                    i += 1;
                    println!("Wrote sample {:?}", i);

                    use std::io::{self, Write};
                    let _ = io::stdout().flush();
                }

                Ok(gstreamer::FlowSuccess::Ok)
            })
            .build(),
    );

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
