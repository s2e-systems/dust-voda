use dust_dds::{
    domain::domain_participant_factory::DomainParticipantFactory,
    infrastructure::{qos::QosKind, status::NO_STATUS},
};
use gstreamer::prelude::*;

include!("../target/idl/video_dds.rs");

fn main() -> Result<(), gstreamer::glib::Error> {
    gstreamer::init()?;

    let domain_id = 0;
    let participant_factory = DomainParticipantFactory::get_instance();
    participant_factory
        .set_configuration(
            dust_dds::configuration::DustDdsConfigurationBuilder::new()
                .fragment_size(60000)
                .udp_receive_buffer_size(Some(60000 * 46))
                .interface_name(Some("Wi-Fi".to_string()))
                .build()
                .unwrap(),
        )
        .unwrap();
    let participant = participant_factory
        .create_participant(domain_id, QosKind::Default, None, NO_STATUS)
        .unwrap();

    let topic = participant
        .create_topic::<Video>(
            "VideoStream",
            "VideoStream",
            QosKind::Default,
            None,
            NO_STATUS,
        )
        .unwrap();

    let publisher = participant
        .create_publisher(QosKind::Default, None, NO_STATUS)
        .unwrap();

    let writer = publisher
        .create_datawriter(&topic, QosKind::Default, None, NO_STATUS)
        .unwrap();

    let pipeline = gstreamer::parse_launch(
        r#"autovideosrc ! video/x-raw,framerate=[1/1,25/1],width=[1,1280],height=[1,720] ! tee name=t ! queue leaky=2 ! videoconvert ! openh264enc complexity=0 ! appsink name=appsink  t. ! queue leaky=2 ! taginject tags="title=Publisher" ! autovideosink"#
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
                    io::stdout().flush().ok();
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

    Ok(())
}
