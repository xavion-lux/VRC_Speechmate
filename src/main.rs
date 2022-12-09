use std::{time::{Duration}, sync::{Arc, Mutex}, net::{SocketAddrV4, UdpSocket}, str::FromStr};
use cpal::{traits::{DeviceTrait, HostTrait, StreamTrait}, {ChannelCount}, SampleFormat};
use dasp::{sample::ToSample, Sample};
use vosk::{DecodingState, Model, Recognizer};
use rosc::{encoder, OscMessage, OscPacket, OscType};

fn main() {
    // get default input device
    let device = cpal::default_host()
        .default_input_device()
        .expect("Failed to get default input device");
    println!("Default input device: {:?}", device.name());

    // get default input config
    let config = device
        .default_input_config()
        .expect("Failed to get default input config");
    println!("Default input config: {:?}", config);

    let channels = config.channels();

    // get model
    let model = Model::new("./models/vosk-model-fr-0.22").expect("Failed to create model");
    let mut recognizer = Recognizer::new(&model, config.sample_rate().0 as f32).expect("Failed to create recognizer");

    recognizer.set_max_alternatives(0);
    recognizer.set_words(true);
    recognizer.set_partial_words(true);

    let recognizer = Arc::new(Mutex::new(recognizer));

    let recognizer_clone = recognizer.clone();
    
    // build input stream
    let stream = match config.sample_format() {
        SampleFormat::F32 => device.build_input_stream(
            &config.into(),
            move |data: &[f32], _| recognize(&mut recognizer_clone.lock().unwrap(), data, channels),
            move |err| eprintln!("an error occurred on stream: {}", err),
        ),
        SampleFormat::U16 => device.build_input_stream(
            &config.into(),
            move |data: &[u16], _| recognize(&mut recognizer_clone.lock().unwrap(), data, channels),
            move |err| eprintln!("an error occurred on stream: {}", err),
        ),
        SampleFormat::I16 => device.build_input_stream(
            &config.into(),
            move |data: &[i16], _| recognize(&mut recognizer_clone.lock().unwrap(), data, channels),
            move |err| eprintln!("an error occurred on stream: {}", err),
        ),
    }
    .expect("Could not build stream");

    let addr = SocketAddrV4::from_str("127.0.0.1:9000").unwrap();
    let sock = UdpSocket::bind("127.0.0.1:8000").unwrap();

    let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
        addr: "/chatbox/input".to_string(),
        args: vec![OscType::String("STT Initialized".to_string()), OscType::Bool(true)],
    }))
    .unwrap();

     sock.send_to(&msg_buf, addr).unwrap();

    loop {
        stream.play().expect("Could not play stream");
        std::thread::sleep(Duration::from_secs(5));
        stream.pause().expect("Could not pause stream");

        let result = recognizer.lock().unwrap().final_result().single().unwrap().text.to_string();

        if result == ""
        {
            continue;
        }

        println!("{:#?}", result);

        let msg_buf = encoder::encode(&OscPacket::Message(OscMessage {
            addr: "/chatbox/input".to_string(),
            args: vec![OscType::String(result), OscType::Bool(true)],
        }))
        .unwrap();

        sock.send_to(&msg_buf, addr).unwrap();
    }
}

fn recognize<T: Sample + ToSample<i16>>(
    recognizer: &mut Recognizer,
    data: &[T],
    channels: ChannelCount,
) {
    let data: Vec<i16> = data.iter().map(|v| v.to_sample()).collect();
    let data = if channels != 1 {
        stereo_to_mono(&data)
    } else {
        data
    };

    let state = recognizer.accept_waveform(&data);
    match state {
        DecodingState::Running => {
            //println!("partial: {:#?}", recognizer.partial_result());
        }
        DecodingState::Finalized => {
            // Result will always be multiple because we called set_max_alternatives
            //println!("result: {:#?}", recognizer.result().multiple().unwrap());
        }
        DecodingState::Failed => eprintln!("error"),
    }
}

pub fn stereo_to_mono(input_data: &[i16]) -> Vec<i16> {
    let mut result = Vec::with_capacity(input_data.len() / 2);
    result.extend(
        input_data
            .chunks_exact(2)
            .map(|chunk| chunk[0] / 2 + chunk[1] / 2),
    );

    result
}