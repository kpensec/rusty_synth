extern crate sdl2;
extern crate rand;
extern crate gl;
#[macro_use]
extern crate serde_derive;
extern crate serde_yaml;
#[macro_use]
extern crate imgui;
extern crate imgui_sdl2;
extern crate imgui_opengl_renderer;
extern crate libloading as lib;

mod ui;
mod utils;
mod synth;
mod platform;
mod render;

#[macro_use] use std::format;

// TODO lookup for some virtual fs crates?
use std::collections::{HashSet, HashMap};
use std::{mem, str, ptr};
use std::ffi::CString;
use std::os::raw::c_void;

fn call_dynamic(lib: &lib::Library) -> lib::Result<u32> {
    unsafe {
        let func: lib::Symbol<unsafe extern fn() -> u32> = lib.get(b"foo")?;
        Ok(func())
    }
}

// TODO should be on the platform layer code
use sdl2::event::{Event};
use sdl2::keyboard::Keycode;
use sdl2::audio::{AudioSpecDesired};

// TODO same
use gl::types::*;


use utils::{Unit, Newable};
use render::gl_utils::make_program;
use synth::{Synthesizer, Oscillator};


// mod RessourceManager
fn load_obj(filepath: &str) -> Vec<f32> {
    let mut result : Vec<f32> = Vec::<f32>::new();

    platform::io::load_asset(filepath, |line: &String| {
        match line.get(0..1) {
            Some("#") => return (),
            _ => {}

        }

        for s in line.trim().split(" ") {
            if s != "" {
                result.push(s.parse::<f32>().unwrap());
            }
        }
    });

    result
}

pub fn load_ressource<RType, F>(filepath: &str, mut func: F) -> RType
    where F : FnMut(&String, &mut RType) -> (),
          RType : Newable {

    let mut ressource = RType::new();
    platform::io::load_asset(filepath, |line: &String| {
        // skip comment
        match line.get(0..1) {
            Some("#") => return,
            _ => {}
        }

        func(line, &mut ressource);
    });
    ressource
}

type Keymap = HashMap<sdl2::keyboard::Keycode, usize>;

impl Newable for Keymap {
    fn new() -> Self {
        HashMap::new()
    }
}

pub fn load_keybind(filepath: &str) -> Keymap {
    load_ressource(filepath, |line: &String, keymap: &mut Keymap| {
        let array : Vec<&str> = line.trim().split(" ").collect();
        let keyname = array[0];
        let note = array[1];

        // TODO change this with a logger function?
        println!("{}, {}", keyname, note);

        keymap.insert(
            sdl2::keyboard::Keycode::from_name(keyname).unwrap(),
            note.parse::<i32>().unwrap() as usize
            );
    })
}

pub struct DataBufferTest {
    pub x: i32
}

#[derive(Debug, Serialize, Deserialize)]
struct ProgramDef {
    vertex: String,
    fragment: String,
}

fn load_instrument(path: &str) -> synth::Instrument {
    serde_yaml::from_str::<synth::Instrument>(
        &platform::io::read_file(path)
    ).unwrap()
}

#[derive(Serialize, Deserialize, Clone, Copy)]
pub struct Color {
    r: Unit,
    g: Unit,
    b: Unit,
}

impl From<[Unit; 3]> for Color {
    fn from(array: [Unit; 3]) -> Self {
        Color {
            r: array[0],
            g: array[1],
            b: array[2],
        }
    }
}

impl Into<[Unit; 3]> for Color {
    fn into(self) -> [Unit; 3] {
        [self.r, self.g, self.b]
    }
}

impl Color {
    fn into_array(self) -> [Unit; 3] {
        self.into()
    }
}

fn main() {

    let mut bg_color = Color{r: 0.2, g: 0.4, b: 0.2};
    let window_cfg : platform::WindowCfg =
      serde_yaml::from_str(&platform::io::read_file("data/cfg/window.yml")).unwrap();
    println!("window cfg: {:?}", window_cfg);
    let mut systems = platform::init(window_cfg);

    let mut lib : Vec<lib::Library> = Vec::new();
    lib.push(lib::Library::new("dylibtest/target/debug/libdylibtest.so").unwrap());

    // need to keep a ref to loaded pointer!
    let mut last_frame_time = 0 as u32;
    let desired_spec = AudioSpecDesired {
        freq: Some(2*22_050),
        channels: Some(2),
        samples: None
    };

    const INSTRUMENT_PATH : &'static str = "data/assets/instrument/test.yml";
    let mut edited_instrument = load_instrument(INSTRUMENT_PATH);
    let mut device = systems.audio.open_playback(None, &desired_spec, |spec| {
        println!("{:?}", spec);
        let mut synth = Synthesizer::new(spec.freq);
        synth.set_instrument(edited_instrument.clone());
        synth

    }).unwrap();

    device.resume();

    // TODO rework keyboard handling API!
    let mut prev_keys = HashSet::new();
    let keyboard_notes = load_keybind("data/cfg/keybind.txt");

    let mut frame_count = 10;
    let mut frame_count_time = 0.0;
    let mut frame_per_sec = 60.0;

    let vertex_data = load_obj("data/assets/triangle.obj");

    // TODO abstract gl object loading!
    // TODO gl program in yaml seems ok
    let program_def: ProgramDef = serde_yaml::from_str(&platform::io::read_file("data/assets/sprite.yml")).unwrap();
    println!("{:?}", program_def);

    let program = make_program(&program_def.vertex, &program_def.fragment);
    let mut vbo = 0;
    let mut vao = 0;
    unsafe { // uploading gl data
        gl::GenVertexArrays(1, &mut vao);
        gl::BindVertexArray(vao);

        gl::BindVertexArray(vao);
        gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
        gl::GenBuffers(1, &mut vbo);
        gl::BindBuffer(gl::ARRAY_BUFFER, vbo);
        gl::BufferData(
            gl::ARRAY_BUFFER,
            (vertex_data.len() * mem::size_of::<GLfloat>()) as GLsizeiptr,
            vertex_data.as_ptr() as *const gl::types::GLvoid,
            gl::STATIC_DRAW
        );
        // Use shader program
        gl::UseProgram(program);
        gl::BindFragDataLocation(program, 0, CString::new("out_color").unwrap().as_ptr());

        // Specify the layout of the vertex data
        let pos_attr = gl::GetAttribLocation(program, CString::new("position").unwrap().as_ptr());
        let color_attr = gl::GetAttribLocation(program, CString::new("color").unwrap().as_ptr());

        let stride = 6 * mem::size_of::<GLfloat>() as GLsizei;
        gl::EnableVertexAttribArray(pos_attr as GLuint);
        gl::VertexAttribPointer(pos_attr as GLuint, 3, gl::FLOAT, gl::FALSE as GLboolean, stride, ptr::null());
        gl::EnableVertexAttribArray(color_attr as GLuint);
        gl::VertexAttribPointer(color_attr as GLuint, 3, gl::FLOAT, gl::FALSE as GLboolean, stride, (3 * mem::size_of::<GLfloat>()) as *const c_void);
    }

    // TODO meh dirty
    let tri_number = vertex_data.len() as i32 / 6;

    let mut imgui = imgui::ImGui::init();
    imgui.set_ini_filename(Some(imgui::ImString::new("data/cfg/imgui.ini")));
    let mut imgui_sdl2 = imgui_sdl2::ImguiSdl2::new(&mut imgui);

    let renderer = imgui_opengl_renderer::Renderer::new(&mut imgui, |s| systems.video.gl_get_proc_address(s) as _);

    let mut display_quit = false;
    let main_loop = || {
        let new_frame_time = systems.timer.ticks();
        let eps = (new_frame_time - last_frame_time) as f32 / 1000.0;
        last_frame_time = new_frame_time;

        for event in systems.event.poll_iter() {
            // TODO use a message queue to dispatch event to subsystems
            imgui_sdl2.handle_event(&mut imgui, &event);
            if imgui_sdl2.ignore_event(&event) {
                continue;
            }
            match event {
                Event::Quit {..} | Event::KeyDown { keycode: Some(Keycode::Escape), .. } => {
                    if display_quit {
                        platform::exit_application();
                    }
                    display_quit = true;
                },
                Event::KeyDown { keycode: Some(Keycode::Left), ..} => {
                },
                Event::KeyDown { keycode: Some(Keycode::Right), ..} => {
                },
                Event::KeyDown { keycode: Some(Keycode::Up), ..} => {
                },
                Event::KeyDown { keycode: Some(Keycode::Down), ..} => {
                },
                Event::KeyDown { keycode: Some(Keycode::F1), ..} => {
                    let mut lock = device.lock();
                    (*lock).toggle_audio();
                },
                Event::KeyDown { keycode: Some(Keycode::F4), ..} => {
                    println!("trying to dycall...");
                    println!("dycall result: {}", call_dynamic(&lib.get(0).unwrap()).unwrap());
                },
                Event::KeyDown { keycode: Some(Keycode::F2), ..} => {
                    std::process::Command::new("cargo")
                        .current_dir("./dylibtest")
                        .args(&["build"]).status().expect("...");

                    lib.swap_remove(0);
                    lib.push(lib::Library::new("dylibtest/target/debug/libdylibtest.so").unwrap());
                    println!("lib size: {}",lib.len());
                },
                Event::KeyDown { keycode: Some(Keycode::F3), ..} => {
                    let instrument = load_instrument(INSTRUMENT_PATH);
                    println!("instr: {:?}", instrument);
                    (*device.lock()).set_instrument(instrument);
                }
                Event::KeyDown { keycode: Some(Keycode::KpEnter), ..} => {
                    let mut lock = device.lock();
                    println!("volume -> {}", (*lock).get_volume());
                },
                Event::KeyDown { keycode: Some(Keycode::KpMinus), ..} => {
                    let mut lock = device.lock();
                    (*lock).set_volume(-0.1);

                },
                Event::KeyDown { keycode: Some(Keycode::KpPlus), ..} => {
                    let mut lock = device.lock();
                    (*lock).set_volume(0.1);
                },
                _ => {}
            }
        }

        { // TODO wrap me?
            let keys = systems.event.keyboard_state()
                .pressed_scancodes()
                .filter_map(Keycode::from_scancode)
                .collect();

            let new_keys = &keys - &prev_keys;
            let old_keys = &prev_keys - &keys;

            for key in new_keys {
                match keyboard_notes.get(&key) {
                    Some(i) => {
                        let mut lock = device.lock();
                        (*lock).start_note(*i);
                    }
                    _ => {}
                }
            }

            for key in old_keys {
                match keyboard_notes.get(&key) {
                    Some(i) => {
                        let mut lock = device.lock();
                        (*lock).release_note(*i);
                    }
                    _ => {}
                }
            }

            prev_keys = keys;
        }


        { // fps counter
            frame_count_time += eps;
            frame_count -= 1;
            if frame_count == 0 {
                if frame_count_time == 0.0 {
                    println!("divided by zero!");
                }
                frame_count = 60;
                frame_per_sec =  frame_count as f32 / frame_count_time ;
                frame_count_time = 0.0;
            }
        }

        unsafe {
            systems.window.gl_set_context_to_current().unwrap();
            gl::ClearColor(bg_color.r, bg_color.g, bg_color.b, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            // gl::DrawArrays(gl::TRIANGLES, 0, tri_number);
        }

        // ui building:
        let ui = imgui_sdl2.frame(&systems.window, &mut imgui, &systems.event);

        if display_quit {
            ui.open_popup(im_str!("Quit ?"));
        }

        // TODO should be a modal_popup aka fork imgui-sdl2 for handling higher version of imgui
        ui.popup(im_str!("Quit ?"), || {
            ui.same_line(0.0);
            if ui.small_button(im_str!("Yes")) {
                platform::exit_application();
                println!("Application exited!");
            }
            ui.same_line(0.0);
            if ui.small_button(im_str!("No")) {
                display_quit = false;
                ui.close_current_popup();
            }
        });

        // TODO render all osc ? (it need to rethink the design of synthesizer...)
        ui.window(im_str!("sound display")).build(|| {
            let lock = device.lock();
            let buf = lock.get_debug_buffer();
            ui.plot_lines(im_str!("curve"), &buf)
              .graph_size((500.0, 100.0))
              .build();
        });

        // osc ui
        ui.window(im_str!("instrument settings")).build(|| {
            let mut current_volume = device.lock().get_volume();
            if ui.drag_float(im_str!("master volume"), &mut current_volume)
              .min(0.0)
              .max(1.0)
              .build() {
                (*(device.lock())).set_volume(current_volume);
            }
            let mut osc_to_remove : Vec<usize> = vec![];
            let mut osc_idx : usize = 0;
            for osc in edited_instrument.get_vec_mut().iter_mut() {
                ui.push_id(im_str!("osc_{}", osc_idx));
                ui.group(|| {
                    let mut function_idx = osc.osc_func as i32;
                    ui.drag_int(im_str!("function"), &mut function_idx).min(0).max(3).build();
                    osc.osc_func = function_idx as usize;
                    ui.input_float(im_str!("amp_{}", osc_idx), &mut osc.osc_amp).build();
                    let r = ui.input_int(im_str!("note_offset_{}", osc_idx), &mut osc.osc_note_offset).build();
                    if ui.small_button(im_str!("remove_{}", osc_idx)) {
                        osc_to_remove.push(osc_idx);
                    }
                    osc_idx = osc_idx + 1;
                });
                ui.pop_id();
            }

            if ui.small_button(im_str!("add")) {
                edited_instrument.get_vec_mut().push(Oscillator{
                    osc_func: 0,
                    osc_amp: 1.0,
                    osc_note_offset: 0,
                    lfo_func: 0,
                    lfo_amp: 0.0,
                    lfo_freq: 0.0
                });
                (*device.lock()).set_instrument(edited_instrument.clone());
            }

            ui.same_line(0.0);
            if ui.small_button(im_str!("load")) {
                (*device.lock()).set_instrument(edited_instrument.clone());
            }
            ui.same_line(0.0);
            if ui.small_button(im_str!("save")) {
                let content = serde_yaml::to_string(&edited_instrument).unwrap();
                platform::io::write_file(INSTRUMENT_PATH, &content);
            }



            // TODO removed oscs
            for elem in osc_to_remove.iter() {
                edited_instrument.get_vec_mut().swap_remove(*elem);
            }
            if ! osc_to_remove.is_empty() {
                (*device.lock()).set_instrument(edited_instrument.clone());
            }
        });

        ui.window(im_str!("video debug")).build(|| {
            ui.text(im_str!("FPS: {}", ui.framerate()));
        });

        ui.window(im_str!("theme settings")).build(|| {
            let mut array = bg_color.into_array();
            if ui.color_picker(im_str!("bg color"), &mut array).build() {
                bg_color = Color::from(array);
            }
        });

        renderer.render(ui);

        systems.window.gl_swap_window();
        platform::sleep();
    };

    platform::start_loop(main_loop);

    unsafe {
        gl::DeleteProgram(program);
        gl::DeleteBuffers(1, &vbo);
        gl::DeleteVertexArrays(1, &vao);
    }
}

