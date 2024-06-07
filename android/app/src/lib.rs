extern crate gstreamer;
use dust_dds::{
    domain::domain_participant_factory::DomainParticipantFactory,
    infrastructure::{qos::QosKind, status::NO_STATUS},
};
use gstreamer::{prelude::*, DebugCategory, DebugLevel, DebugMessage};
use gstreamer_video_sys::GstVideoOverlay;
use jni::{
    objects::{GlobalRef, JClass, JObject, JValueGen},
    sys::jint,
    JNIEnv, JavaVM,
};
use ndk_sys::android_LogPriority;
use std::{ffi::CString, fmt::Display};

#[derive(Debug)]
struct ErrorMessage {
    src: String,
    error: String,
    debug: Option<String>,
    source: glib::Error,
}
impl Display for ErrorMessage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "src: {}, error: {}, debug: {}, orig error: {}",
            self.src,
            self.error,
            self.debug.as_ref().unwrap_or(&"".to_string()),
            self.source
        )
    }
}
struct VodaError(String);

impl std::fmt::Display for VodaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<glib::Error> for VodaError {
    fn from(value: glib::Error) -> Self {
        VodaError(value.to_string())
    }
}
impl From<gstreamer::StateChangeError> for VodaError {
    fn from(value: gstreamer::StateChangeError) -> Self {
        VodaError(value.to_string())
    }
}
impl From<ErrorMessage> for VodaError {
    fn from(value: ErrorMessage) -> Self {
        VodaError(value.to_string())
    }
}

#[derive(Debug, dust_dds::topic_definition::type_support::DdsType)]
pub struct Video {
    pub user_id: i16,
    pub frame_num: i32,
    pub frame: Vec<u8>,
}

static mut JAVA_VM: Option<JavaVM> = None;
static mut CLASS_LOADER: Option<GlobalRef> = None;

static mut MY_VIDEO_PIPELINE: Option<gstreamer::Pipeline> = None;

fn android_log_write(prio: android_LogPriority, tag: &str, msg: &str) {
    let tag_c = CString::new(tag).expect("tag str not converted to CString");
    let msg_c = CString::new(msg).expect("msg str not converted to CString");
    unsafe {
        ndk_sys::__android_log_write(
            prio.0 as std::os::raw::c_int,
            tag_c.as_ptr(),
            msg_c.as_ptr(),
        );
    }
}

fn glib_print_handler(msg: &str) {
    android_log_write(android_LogPriority::ANDROID_LOG_INFO, "GLib+stdout", msg);
}

fn glib_printerr_handler(msg: &str) {
    android_log_write(android_LogPriority::ANDROID_LOG_ERROR, "GLib+stderr", msg);
}

fn glib_log_handler(domain: Option<&str>, level: glib::LogLevel, msg: &str) {
    let prio = match level {
        glib::LogLevel::Error => android_LogPriority::ANDROID_LOG_ERROR,
        glib::LogLevel::Critical => android_LogPriority::ANDROID_LOG_ERROR,
        glib::LogLevel::Warning => android_LogPriority::ANDROID_LOG_WARN,
        glib::LogLevel::Message => android_LogPriority::ANDROID_LOG_INFO,
        glib::LogLevel::Info => android_LogPriority::ANDROID_LOG_INFO,
        glib::LogLevel::Debug => android_LogPriority::ANDROID_LOG_DEBUG,
    };
    let tag = format!("Glib+{}", domain.unwrap_or(""));
    android_log_write(prio, &tag, msg);
}

fn debug_logcat(
    category: DebugCategory,
    level: DebugLevel,
    file: &gstreamer::glib::GStr,
    function: &gstreamer::glib::GStr,
    line: u32,
    object: Option<&gstreamer::log::LoggedObject>,
    message: &DebugMessage,
) {
    if level > category.threshold() {
        return;
    }
    let prio = match level {
        DebugLevel::Error => android_LogPriority::ANDROID_LOG_ERROR,
        DebugLevel::Warning => android_LogPriority::ANDROID_LOG_WARN,
        DebugLevel::Info => android_LogPriority::ANDROID_LOG_INFO,
        DebugLevel::Debug => android_LogPriority::ANDROID_LOG_DEBUG,
        _ => android_LogPriority::ANDROID_LOG_VERBOSE,
    };

    let tag = format!("GStreamer+{}", category.name());
    match object {
        Some(obj) => {
            let label = obj.to_string();
            let msg = format!(
                "{} {}:{}:{}:{} {}",
                gstreamer::get_timestamp(),
                file,
                line,
                function,
                label,
                message.get().unwrap()
            );
            android_log_write(prio, &tag, &msg);
        }
        None => {
            let msg = format!(
                "{} {}:{}:{} {}",
                gstreamer::get_timestamp(),
                file,
                line,
                function,
                message.get().unwrap()
            );
            android_log_write(prio, &tag, &msg);
        }
    }
}

/// # Safety
///
/// Must use globals
#[no_mangle]
pub unsafe extern "C" fn gst_android_get_java_vm() -> *const jni::sys::JavaVM {
    match JAVA_VM.as_ref() {
        Some(vm) => vm.get_java_vm_pointer(),
        None => std::ptr::null(),
    }
}

/// This functions is searched by name by the androidmedia plugin. It must hence be present
/// even if it appears to be unused
/// # Safety
///
/// Must use globals
#[no_mangle]
pub unsafe extern "C" fn gst_android_get_application_class_loader() -> jni::sys::jobject {
    match CLASS_LOADER.as_ref() {
        Some(o) => o.as_raw(),
        None => std::ptr::null_mut(),
    }
}

/// # Safety
///
/// Must use ndk
#[no_mangle]
pub unsafe extern "C" fn Java_tw_mapacode_androidsink_SurfaceHolderCallback_nativeSurfaceInit(
    env: JNIEnv,
    _: JClass,
    surface: jni::sys::jobject,
) {
    if let Some(pipeline) = MY_VIDEO_PIPELINE.as_ref() {
        let overlay = pipeline.by_interface(gstreamer_video::VideoOverlay::static_type());
        if let Some(overlay) = &overlay {
            let overlay = overlay.as_ptr() as *mut GstVideoOverlay;
            let native_window = ndk_sys::ANativeWindow_fromSurface(env.get_raw(), surface);
            gstreamer_video_sys::gst_video_overlay_set_window_handle(
                overlay,
                native_window as usize,
            )
        }
    } else {
        android_log_write(
            android_LogPriority::ANDROID_LOG_ERROR,
            "VoDA",
            "Pipeline not initialized yet",
        );
    }
}

/// # Safety
///
/// Must use ndk
#[no_mangle]
pub unsafe extern "C" fn Java_tw_mapacode_androidsink_SurfaceHolderCallback_nativeSurfaceFinalize(
    env: JNIEnv,
    _: JClass,
    surface: jni::sys::jobject,
) {
    ndk_sys::ANativeWindow_release(ndk_sys::ANativeWindow_fromSurface(env.get_raw(), surface));
}

/// # Safety
///
/// Must use globals
#[no_mangle]
pub unsafe extern "C" fn Java_org_freedesktop_gstreamer_GStreamer_nativeInit(
    mut env: JNIEnv,
    _: JClass,
    context: JObject,
) {
    // Store context and class cloader.
    match env.call_method(&context, "getClassLoader", "()Ljava/lang/ClassLoader;", &[]) {
        Ok(loader) => match loader {
            JValueGen::Object(obj) => {
                CLASS_LOADER = env.new_global_ref(obj).ok();
                match env.exception_check() {
                    Ok(value) => {
                        if value {
                            env.exception_describe().unwrap();
                            env.exception_clear().unwrap();
                            return;
                        }
                    }
                    Err(e) => {
                        android_log_write(
                            android_LogPriority::ANDROID_LOG_ERROR,
                            "VoDA",
                            &format!("{}", e),
                        );
                        return;
                    }
                }
            }
            _ => {
                android_log_write(
                    android_LogPriority::ANDROID_LOG_ERROR,
                    "VoDA",
                    "Could not get class loader",
                );
                return;
            }
        },
        Err(e) => {
            android_log_write(
                android_LogPriority::ANDROID_LOG_ERROR,
                "VoDA",
                &format!("{}", e),
            );
            return;
        }
    }

    glib::set_print_handler(glib_print_handler);
    glib::set_printerr_handler(glib_printerr_handler);
    glib::log_set_default_handler(glib_log_handler);

    gstreamer::log::set_active(true);
    gstreamer::log::set_default_threshold(gstreamer::DebugLevel::Warning);
    gstreamer::log::remove_default_log_function();
    gstreamer::log::add_log_function(debug_logcat);

    match gstreamer::init() {
        Ok(_) => { /* Do nothing. */ }
        Err(e) => {
            android_log_write(
                android_LogPriority::ANDROID_LOG_ERROR,
                "VoDA",
                &format!("GStreamer initialization failed: {}", e),
            );
            match env.find_class("java/lang/Exception") {
                Ok(c) => {
                    env.throw_new(c, &format!("GStreamer initialization failed: {}", e))
                        .ok();
                }
                Err(e) => {
                    android_log_write(
                        android_LogPriority::ANDROID_LOG_ERROR,
                        "VoDA",
                        &format!("Could not get Exception class: {}", e),
                    );
                    return;
                }
            }
            return;
        }
    }

    extern "C" {
        fn gst_plugin_videotestsrc_register();
        fn gst_plugin_autodetect_register();
        fn gst_plugin_opengl_register();
        fn gst_plugin_app_register();
        fn gst_plugin_coreelements_register();
        fn gst_plugin_openh264_register();
        fn gst_plugin_videoconvertscale_register();
        fn gst_plugin_androidmedia_register();
    }

    gst_plugin_videotestsrc_register();
    gst_plugin_autodetect_register();
    gst_plugin_opengl_register();
    gst_plugin_app_register();
    gst_plugin_coreelements_register();
    gst_plugin_openh264_register();
    gst_plugin_videoconvertscale_register();
    gst_plugin_androidmedia_register();
}

/// # Safety
///
/// Must use globals
#[no_mangle]
unsafe extern "C" fn Java_tw_mapacode_androidsink_MainActivity_nativeRun(_env: JNIEnv, _: JClass) {
    if MY_VIDEO_PIPELINE.as_ref().is_none() {
        MY_VIDEO_PIPELINE = create_pipeline().ok();
        std::thread::spawn(move || match MY_VIDEO_PIPELINE.as_ref() {
            Some(pipeline) => match main_loop(pipeline) {
                Ok(_) => (),
                Err(e) => android_log_write(
                    android_LogPriority::ANDROID_LOG_ERROR,
                    "VoDA",
                    &format!("main_loop error: {}", e),
                ),
            },
            None => android_log_write(
                android_LogPriority::ANDROID_LOG_ERROR,
                "VoDA",
                "create_pipeline error",
            ),
        });
    }
}

/// # Safety
///
/// Must store JAVA_VM
#[no_mangle]
#[allow(non_snake_case)]
unsafe fn JNI_OnLoad(jvm: JavaVM, _reserved: *mut std::os::raw::c_void) -> jint {
    let mut env: JNIEnv;
    match jvm.get_env() {
        Ok(v) => {
            env = v;
        }
        Err(e) => {
            android_log_write(
                android_LogPriority::ANDROID_LOG_ERROR,
                "VoDA",
                &format!("Could not retrieve JNIEnv, error: {}", e),
            );
            return 0;
        }
    }

    let version: jint;
    match env.get_version() {
        Ok(v) => {
            version = v.into();
            android_log_write(
                android_LogPriority::ANDROID_LOG_INFO,
                "VoDA",
                &format!("JNI Version: {:#x?}", version),
            );
        }
        Err(e) => {
            android_log_write(
                android_LogPriority::ANDROID_LOG_ERROR,
                "VoDA",
                &format!("Could not retrieve JNI version, error: {}", e),
            );
            return 0;
        }
    }

    match env.find_class("org/freedesktop/gstreamer/GStreamer") {
        Ok(_) => {}
        Err(e) => {
            android_log_write(
                android_LogPriority::ANDROID_LOG_ERROR,
                "VoDA",
                &format!(
                    "Could not retreive class org.freedesktop.gstreamer.GStreamer, error: {}",
                    e
                ),
            );
            return 0;
        }
    }
    JAVA_VM = Some(jvm);
    version
}

fn create_pipeline() -> Result<gstreamer::Pipeline, VodaError> {
    let pipeline_element = gstreamer::parse::launch("ahcsrc ! video/x-raw,format=NV21,framerate=[1/1,25/1],width=[1,1280],height=[1,720] ! tee name=t ! queue leaky=2 ! glimagesink t. ! queue leaky=2 ! videoconvert ! openh264enc complexity=0 ! appsink name=appsink")?;

    let participant = DomainParticipantFactory::get_instance()
        .create_participant(0, QosKind::Default, None, NO_STATUS)
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

    let pipeline = pipeline_element
        .dynamic_cast::<gstreamer::Pipeline>()
        .expect("Pipeline is expected to be a bin");
    let sink = pipeline
        .by_name("appsink")
        .ok_or(VodaError("appsink missing".to_string()))?;
    let appsink = sink
        .dynamic_cast::<gstreamer_app::AppSink>()
        .expect("Sink element is expected to be an appsink!");

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
                }
                Ok(gstreamer::FlowSuccess::Ok)
            })
            .build(),
    );

    Ok(pipeline)
}

fn main_loop(pipeline: &gstreamer::Pipeline) -> Result<(), VodaError> {
    pipeline.set_state(gstreamer::State::Playing)?;

    let bus = pipeline
        .bus()
        .expect("Pipeline without bus. Shouldn't happen!");

    for msg in bus.iter_timed(gstreamer::ClockTime::NONE) {
        match msg.view() {
            gstreamer::MessageView::Eos(..) => break,
            gstreamer::MessageView::Error(err) => {
                pipeline.set_state(gstreamer::State::Null)?;
                return Err(ErrorMessage {
                    src: msg
                        .src()
                        .map(|s| String::from(s.path_string()))
                        .unwrap_or_else(|| String::from("None")),
                    error: err.error().to_string(),
                    debug: err.debug().map(|s| s.to_string()),
                    source: err.error(),
                }
                .into());
            }
            _ => (),
        }
    }
    pipeline.set_state(gstreamer::State::Null)?;

    Ok(())
}
