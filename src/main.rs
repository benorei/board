use board::{Arrival, fetch_with_retry, format_time, recalculate_and_filter};
use slint::{Model, VecModel};
use slint::Image;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
// use std::path::Path;
use std::time::Duration;
use include_assets::{NamedArchive, include_dir};
#[cfg(feature = "framebuffer")]
use {
    slint_backend_linuxfb::LinuxFbPlatformBuilder,
    std::fs::File,
};
#[cfg(feature = "windowed")]
use std::io;
use {
    std::io::{BufReader, BufRead},
    std::thread,
    tokio::sync::mpsc,
};

static ARCHIVE: std::sync::LazyLock<NamedArchive> = std::sync::LazyLock::new(|| {
    return NamedArchive::load(include_dir!("assets"));
});



slint::slint! {
import {LineEdit, TextEdit, StandardButton, HorizontalBox} from "std-widgets.slint";
export struct ArrivalData {
    destination: string,
    time: string,
    route_color: color,
    route_label: string,
    real_time: bool,
    rail: bool,
    linebullet_text_color: color,
    has_linebullet_img: bool,
    linebullet_img: image,
}

export component MainUI inherits Window {
    title: "Departures Board";
    default-font-family: "Berthold Akzidenz Grotesk";
    in-out property <[ArrivalData]> arrivals;
    in-out property <string> current_time;
    in-out property <string> stop_name;
    in-out property <bool> show_loading_screen;

    background: black;
    width: 800px;
    height: 480px;
    no-frame: true;

    Rectangle {
        width: 800px;
        height: 480px;
        background: black;
        visible: show_loading_screen;
        
        Text {
            text: "Loading " + stop_name + "...";
            horizontal-alignment: center;
            vertical-alignment: center;
            color: white;
            font-size: 40px;
        }
    }

    VerticalLayout {
        alignment: start;
        padding-left: 30px;
        padding-right: 30px;
        padding-top: 10px;
        padding-bottom: 30px;
        spacing: 10px;
        visible: !show_loading_screen;

        // Header: time left-aligned, stop name centered
        Rectangle {
            height: 40px;

            // Time on the left
            Text {
                x: 0;
                text: current_time;
                font-size: 24pt;
                font-weight: 800;
                color: white;
                vertical-alignment: center;
            }

            // Stop name centered
            Text {
                text: stop_name;
                font-size: 24pt;
                font-weight: 800;
                color: white;
                horizontal-alignment: center;
                vertical-alignment: center;
                width: 100%;
            }
        }

        for arrival in arrivals: HorizontalLayout {
            spacing: 15px;
            height: 60px;

            // Linebullet
            HorizontalLayout{
                width: 75px;
                Rectangle {}
                // Rectangle {visible: arrival.has_linebullet_img;} // Center image
                Image {
                    visible: arrival.has_linebullet_img;
                    source: arrival.linebullet_img;
                    height: 50px;
                    width: arrival.has_linebullet_img ? 75px : 0px;
                    horizontal-alignment: center;
                    vertical-alignment: center;
                    image-fit: contain;
                } 
                // Rectangle {visible: arrival.has_linebullet_img;} // Center image

                Rectangle {
                    visible: !arrival.has_linebullet_img;
                    width: arrival.has_linebullet_img ? 0px : 75px;
                    height: 50px;
                    background: arrival.route_color;
                    border-radius: arrival.rail ? 25px : 0px;
                    VerticalLayout {
                        Rectangle {} // Make it look centered
                        Text {
                            text: arrival.route_label;
                            font-size: 28pt;
                            font-weight: 800;
                            color: arrival.linebullet-text-color;
                            horizontal-alignment: center;
                            vertical-alignment: center;
                        }
                    }
                }
                Rectangle {}
            }

            // Destination
            Text {
                text: arrival.destination;
                font-size: 30pt;
                font-weight: 800;
                color: white;
                vertical-alignment: center;
                horizontal-alignment: left;
                horizontal-stretch: 1;
                overflow: elide;
            }

            HorizontalLayout {
                padding-bottom: 8px;
                spacing: 7px;
                alignment: end;
                // cross-axis-alignment: end; // 1.18.1 only
                // Not-Realtime Indicator
                VerticalLayout {
                    visible: !arrival.real_time;
                    alignment: end;
                    padding-bottom: 10px;
                    max-width: 25px;
                    Image {
                        source: @image-url("scheduled.svg");
                        // height: 20px;
                        // horizontal-alignment: right;
                        // vertical-alignment: center;
                        colorize: arrival.time=="Now" ? orange : white;
                    }
                }
                // Time
                Text {
                    text: arrival.time;
                    font-size: 25pt;
                    font-weight: 800;
                    color: arrival.time=="Now" ? orange : white;
                    vertical-alignment: bottom;
                    horizontal-alignment: right;
                    // min-width: 100px;
                }
            }
        }
    }
}
}

struct State {
    main_ui: MainUI,
    stop_ids: RefCell<Vec<String>>,
    cache: RefCell<HashMap<String, Vec<Arrival>>>,
    raw_arrivals: RefCell<Vec<Arrival>>,
    clock_offset_ms: Cell<i64>,
    arrivals_model: Rc<VecModel<ArrivalData>>,
}

async fn fetch_arrivals(state: &State) {
    let mut all_arrivals: Vec<Arrival> = Vec::new();

    let stop_ids = state.stop_ids.borrow().clone();
    for stop_id in stop_ids.iter() {
        match fetch_with_retry(stop_id).await {
            Ok((arrivals, clock_offset)) => {
                state.clock_offset_ms.set(clock_offset);
                state.cache.borrow_mut().insert(stop_id.to_owned(), arrivals.clone());
                all_arrivals.extend(arrivals);
            }
            Err(_) => {
                let cached = state.cache.borrow().get(stop_id).cloned();
                if let Some(cached) = cached {
                    all_arrivals.extend(cached.clone());
                }
            }
        }
    }

    *state.raw_arrivals.borrow_mut() = all_arrivals;
    refresh_display(state);
}

fn refresh_display(state: &State) {
    let mut arrivals = state.raw_arrivals.borrow().clone();
    recalculate_and_filter(&mut arrivals, state.clock_offset_ms.get());

    let model = &state.arrivals_model.clone();
    for (i, a) in arrivals.iter().enumerate() {
        let [r, g, b] = a.route_color;
        let text_color = get_readable_text_color(r, g, b);
        let (has_linebullet_img, linebullet_img_src) = get_linebullet_src(a.route_id.clone());
        let data = ArrivalData {
            destination: a.destination.clone().into(),
            time: format_time(a.minutes).replace("m", " min").into(),
            route_color: slint::Color::from_rgb_u8(r, g, b),
            route_label: a.route_label.clone().into(),
            real_time: a.real_time.clone(),
            rail: a.trip_id.clone().contains("Rail"),
            linebullet_text_color: slint::Color::from_rgb_u8(text_color.0, text_color.1, text_color.2),
            // linebullet_src: get_linebullet_src(a.route_id.clone()).into(),
            has_linebullet_img: has_linebullet_img.clone(),
            // linebullet_img: if linebullet_img_src == "" {None} else {Image::load_from_path(linebullet_img_src.into())},
            // linebullet_img: Image::load_from_path(Path::new(linebullet_img_src.as_str())).expect("Invalid linebullet image path"),
            linebullet_img: Image::load_from_svg_data(
                ARCHIVE.get(&linebullet_img_src.as_str()).unwrap()
            ).expect("Invalid linebullet image path"),
        };
        if i < model.row_count() {
            model.set_row_data(i, data);
        } else {
            model.push(data);
        }
    }
    while model.row_count() > arrivals.len() {
        model.remove(model.row_count() - 1);
    }
}

fn main() {
    let stop_ids: Vec<String> = Vec::new();

    // let stop_name = args[0].clone();
    // let stop_ids: Vec<String> = args[1..].to_vec();

    let rt = tokio::runtime::Runtime::new().unwrap();
    let _tokio = rt.enter();

    #[cfg(feature = "framebuffer")]
    {
        let platform = LinuxFbPlatformBuilder::new()
            .with_framebuffer("/dev/fb0")
            .with_input_autodiscovery(true)
            .build()
            .unwrap();
        slint::platform::set_platform(Box::new(platform)).unwrap();
    }

    let arrivals_model = Rc::new(VecModel::<ArrivalData>::default());
    let main_ui = MainUI::new().unwrap();
    main_ui.set_arrivals(arrivals_model.clone().into());

    let state = Rc::new(State {
        main_ui,
        stop_ids: RefCell::new(stop_ids),
        cache: RefCell::new(HashMap::new()),
        raw_arrivals: RefCell::new(Vec::new()),
        clock_offset_ms: Cell::new(0),
        arrivals_model,
    });

    #[cfg(feature = "framebuffer")]
    state.main_ui.set_stop_name("Scan station QR to start".into());

    #[cfg(feature = "windowed")]
    state.main_ui.set_stop_name("Set stop from command line".into());

    // Update clock + refresh display every 10s
    {
        let state = state.clone();
        slint::spawn_local(async move {
            let mut tick = 0u32;
            loop {
                let now = chrono::Local::now();
                state.main_ui.set_current_time(now.format("%H:%M:%S").to_string().into());
                if tick % 10 == 0 {
                    refresh_display(&state);
                }
                tick = tick.wrapping_add(1);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        })
        .unwrap();
    }

    // Sync system clock from HTTP every 5 minutes
    slint::spawn_local(board::clock::run_clock_sync()).unwrap();

    // Fetch from API every 60s
    {
        let state = state.clone();
        slint::spawn_local(async move {
            loop {
                fetch_arrivals(&state).await;
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        })
        .unwrap();
    }

    // Fetch stop information from QR reader or command line
    let (tx, mut rx) = mpsc::unbounded_channel();
    thread::spawn(move || {
        #[cfg(feature = "framebuffer")]
        let scanner = File::open("/dev/ttyUSB0").expect("Couldn't open file");

        #[cfg(feature = "windowed")]
        let scanner = io::stdin();
        let mut reader = BufReader::new(scanner);
        // let mut reader = BufReader::new(reader);

        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => break, // EOF or read error
                Ok(_) => {}
            }
            let line = line
                .trim_start_matches(['\r', '\n'])
                .trim_end_matches(['\r', '\n'])
                .to_owned();
            let info = line.split_once(':');
            if info == None {continue;}
            let info_u = info.unwrap();

            let stop_ids: Vec<String> = info_u.1.split_whitespace().map(str::to_owned).collect();

            if tx.send((info_u.0.to_string(), stop_ids.clone())).is_err() {
                break; // async task has ended
            }


            

            // let info = Vec::from_iter(line.split(":").map(String::from));

            // let stop_name = stop_name.trim_end_matches(['\r', '\n']).to_owned();

            // let mut stops = String::new();
            // match reader.read_line(&mut stops) {
            //     Ok(0) | Err(_) => break,
            //     Ok(_) => {}
            // }

            
        }
    });

    let task_state = state.clone();
    slint::spawn_local(async move {
        while let Some((stop_name, stop_ids)) = rx.recv().await {
            task_state.main_ui.set_show_loading_screen(true);
            task_state.main_ui.set_stop_name(stop_name.into());
            *task_state.stop_ids.borrow_mut() = stop_ids;

            fetch_arrivals(&task_state).await;
            task_state.main_ui.set_show_loading_screen(false);
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    })
    .unwrap();

    state.main_ui.run().unwrap();
}

fn get_readable_text_color(r: u8, g: u8, b: u8) -> (u8, u8, u8) {
    if (r < 127) as u8 + (g < 127) as u8 + (b < 127) as u8 > 2 {
        return (255, 255, 255);
    } else {
        return (0,0,0);
    }
}

fn get_linebullet_src(route_id:String) -> (bool, String) {
    if route_id.starts_with("95_") {return (true, "wsdot.svg".to_string());}
    let svg_path = format!("linebullet/{route_id}.svg");
    let result = ARCHIVE.contains(svg_path.as_str());
    if result {
        return (true, svg_path.to_string());
    } else {
        return (false, "invalid.svg".to_string().to_string());
    }
}