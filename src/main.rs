// (c) 2016-2018 Joost Yervante Damad <joost@damad.be>

#![feature(proc_macro, specialization, const_fn, try_from)]

extern crate cairo;
extern crate clap;
extern crate chrono;
extern crate env_logger;
extern crate pyo3;

extern crate gdk_pixbuf;
extern crate gio;
extern crate glib;
extern crate gtk;

extern crate inotify;
#[macro_use]
extern crate log;
extern crate range;

#[macro_use]
extern crate lazy_static;

extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;


use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::convert::TryFrom;
use std::fs;

use clap::{Arg, App};

use inotify::{WatchMask, Inotify};

use pyo3::{Python, ObjectProtocol, PyList};

use gtk::{WidgetExt, StatusbarExt, TextBufferExt, DialogExt};
use gtk::{NotebookExtManual};
use gtk::{FileChooserDialog, FileChooserAction, FileChooserExt, ResponseType};

use error::MpError;

pub const VERSION: &'static str = env!("CARGO_PKG_VERSION");

const PRELUDEPY:&'static str = include_str!("prelude.py");

#[derive(Debug,Default)]
pub struct DrawState {
    pub bound:element::Bound,
    pub elements:Vec<element::Element>,
}

impl DrawState {
    fn name(&self) -> String {
        for e in &self.elements {
            match e {
                element::Element::Name(x) => {
                    return x.text.txt.clone()
                },
                _ => (),
            }
        }
        "NAME".into()
    }
}

fn main() -> Result<(), MpError> {
    std::env::set_var("RUST_LOG","debug");
    env_logger::init();
    let matches = App::new("madparts")
        .version(VERSION)
        .author("Joost Yervante Damad <joost@damad.be>")
        .about("a functional footprint editor")
        .arg(Arg::with_name("INPUT")
             .help("Sets the python file to use")
             .required(true)
             .index(1))
        .get_matches();

    let filename = matches.value_of("INPUT").unwrap();
    let filepath:PathBuf = Path::new(&filename).canonicalize().unwrap();
    info!("Filename: {}", filepath.display());
    
    let filedir:PathBuf = filepath.parent().unwrap().into();
    info!("Dir: {}", filedir.display());

    if let Err(err) = gtk::init() {
        error!("Failed to initialize GTK.");
        return Err(err.into())
    }

    let mut ino = match Inotify::init() {
        Ok(ino) => ino,
        Err(err) => {
            error!("Failed to initialize INotify");
            return Err(err.into())
        },
    };

    // close_write,moved_to,create indicate the file was possibly messed with
    let _file_watch = ino.add_watch(&filedir, WatchMask::CREATE | WatchMask::MOVED_TO | WatchMask::CLOSE_WRITE).unwrap();

    let draw_state = Arc::new(Mutex::new(DrawState::default()));
    
    let ui = gui::make_gui(&filename, draw_state.clone());
    
    let update_input = Arc::new(AtomicBool::new(true));
    let update_input_timeout_loop = update_input.clone();
    gtk::timeout_add(250, move || {
        let mut buffer = [0; 1024];
        for event in ino.read_events(&mut buffer).unwrap() {
            let mut modified = false;
            if event.name == filepath.file_name() {
                debug!("modified!");
                modified = true;
            }
            if modified {
                update_input_timeout_loop.store(true, Ordering::SeqCst);
                break;
            }
        }
        glib::Continue(true)
    });
    
    ui.show_all();

    let gil = Python::acquire_gil();
    let py = gil.python();

    let sys = py.import("sys")?;
    let version: String = sys.get( "version")?.extract()?;
    
    info!("using python: {}", version);

    py.run(PRELUDEPY,None,None)?;
    // info!("Using prelude: {}", PRELUDEPY);
    info!("prelude loaded.");
    
    loop {
        if ui.want_exit() {
            break;
        }
        if ui.want_save() {
            let d = FileChooserDialog::with_buttons(
                Some("Export kicad file"),
                Some(ui.get_window()),
                FileChooserAction::Save,
                 &[("_Cancel", ResponseType::Cancel), ("_Export", ResponseType::Accept)]
            );
            let filename = {
                let draw_state = draw_state.lock().unwrap();
                format!("{}.kicad_mod", draw_state.name())
            };
            d.set_current_name(filename);
            let res:ResponseType = d.run().into();
            if res == ResponseType::Accept {
                let draw_state = draw_state.lock().unwrap();
                let filename = d.get_filename().unwrap();
                kicad::save(&draw_state.elements, filename)?;
                // TODO: handle failure to save: show error message to user
            }
            d.destroy();
        }
        gtk::main_iteration();
        if update_input.compare_and_swap(true, false, Ordering::SeqCst) {
            ui.statusbar.push(1, "Updating...");
            let data = fs::read_to_string(&filename).unwrap();
            ui.input_buffer.set_text(&data);
            ui.statusbar.pop(1);
            debug!("updated");
            let res = match py.eval(&format!("handle_load_python(\"{}\")", filename), None, None) {
                Ok(res) => res,
                Err(e) => {
                    e.print(py);
                    continue;
                }
            };
            info!("res: {:?}", res);
            let resl:&PyList = res.extract()?;
            let mut draw_state = draw_state.lock().unwrap();
            draw_state.elements.clear();
            let mut failed = false;
            for i in 0..resl.len() {
                let item = resl.get_item(i as isize);
                let gen = item.call_method0("generate")?;
                //info!("gen: {:?}", gen);
                let genl:&PyList = gen.extract()?;
                for j in 0..genl.len() {
                    let item = genl.get_item(j as isize);
                    info!("item: {:?}", item);
                    let json:String = item.extract()?;
                    let x = element::Element::try_from(json)?;
                    info!("x: '{:?}'", x);
                    if let element::Element::PythonError(element::PythonError { message }) = x {
                        let message = message.replace("<string>", &filename);
                        ui.input_buffer.set_text(&message);
                        info!("SET TO ZERO?!");
                        ui.notebook.set_current_page(Some(0));
                        failed = true;
                        break;
                    }
                    draw_state.elements.push(x);
                }
            }
            if failed {
                continue;
            }
            draw_state.bound = element::bound(&draw_state.elements);
            info!("Bound: {:?}", draw_state.bound);
            let mut title = format!("madparts (rustic edition) {} : ", VERSION);
            title.push_str(&draw_state.name());
            ui.set_title(&title);
            ui.statusbar.pop(0);
            ui.statusbar.push(0, &format!("{} ready.", draw_state.name()));
            ui.notebook.set_current_page(Some(1));
            ui.draw();
        }
    }
    Ok(())
}

mod element;
mod error;
mod gui;
mod kicad;
mod settings;
mod util;
